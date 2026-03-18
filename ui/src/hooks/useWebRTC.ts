import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../api';
import type { BitrateLevel, CallChatMessage, CallState, CallType } from '../types';

const ICE_SERVERS: RTCIceServer[] = []; // LAN only — mDNS handles discovery

// A1 — getUserMedia constraints
const VIDEO_CONSTRAINTS: MediaTrackConstraints = {
  width:      { ideal: 1280, min: 640 },
  height:     { ideal: 720,  min: 360 },
  frameRate:  { ideal: 30,   min: 15  },
  facingMode: 'user',
};

const AUDIO_CONSTRAINTS: MediaTrackConstraints = {
  echoCancellation: true,
  noiseSuppression: true,
  autoGainControl:  true,
  sampleRate:       { ideal: 48000 },
  channelCount:     { ideal: 2 },
};

// A4 — Adaptive Bitrate constants
const BITRATE_LEVELS = {
  high:   { video: 2_500_000, audio: 128_000 },
  medium: { video:   800_000, audio: 128_000 },
  low:    { video:   300_000, audio:  64_000 },
} as const;

const RESOLUTION_LEVELS: Record<BitrateLevel, MediaTrackConstraints> = {
  high:   { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
  medium: { width: { ideal: 854  }, height: { ideal: 480 }, frameRate: { ideal: 24 } },
  low:    { width: { ideal: 640  }, height: { ideal: 360 }, frameRate: { ideal: 15 } },
};

interface CallSignalPayload {
  peer_id: string;
  call_id: string;
  signal_json: string;
}

type Signal =
  | { type: 'offer';         sdp: string; call_type: CallType }
  | { type: 'answer';        sdp: string }
  | { type: 'ice_candidate'; candidate: string; sdp_mid: string | null; sdp_mline_index: number | null }
  | { type: 'hangup' }
  | { type: 'reject' };

const INITIAL_STATE: CallState = {
  status: 'idle', peerId: null, peerName: null, callId: null,
  callType: 'audio', isMuted: false, isVideoOff: false,
  isSharingScreen: false, startTime: null,
};

// ── Debug logger ─────────────────────────────────────────────────────────────
const LOG_PREFIX = '[WebRTC]';
let _logSeq = 0;
function wlog(tag: string, msg: string, data?: unknown) {
  const seq = ++_logSeq;
  const ts = new Date().toISOString().slice(11, 23); // HH:MM:SS.mmm
  if (data !== undefined) {
    console.log(`${ts} #${seq} ${LOG_PREFIX}[${tag}] ${msg}`, data);
  } else {
    console.log(`${ts} #${seq} ${LOG_PREFIX}[${tag}] ${msg}`);
  }
}
// ─────────────────────────────────────────────────────────────────────────────

export function useWebRTC(onIncomingCall?: (state: CallState) => void) {
  const [callState, setCallState] = useState<CallState>(INITIAL_STATE);
  const [callMessages, setCallMessages] = useState<CallChatMessage[]>([]);

  const pcRef = useRef<RTCPeerConnection | null>(null);
  const localStreamRef = useRef<MediaStream | null>(null);
  const screenStreamRef = useRef<MediaStream | null>(null);
  const remoteVideoRef = useRef<HTMLVideoElement | null>(null);
  const localVideoRef = useRef<HTMLVideoElement | null>(null);
  const callStateRef = useRef<CallState>(INITIAL_STATE);

  // A2 — disconnected watchdog
  const disconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ICE candidate queue — buffer candidates until setRemoteDescription completes
  const pendingCandidatesRef = useRef<RTCIceCandidateInit[]>([]);
  const remoteDescSetRef = useRef(false);

  // A4 — ABR ref
  const abrRef = useRef<{
    level: BitrateLevel;
    stableSeconds: number;
    intervalId: ReturnType<typeof setInterval> | null;
    graceTicks: number; // skip first N ticks after connected/reconnected
  }>({ level: 'high', stableSeconds: 0, intervalId: null, graceTicks: 3 });

  // B2 — DataChannel ref
  const dataChannelRef = useRef<RTCDataChannel | null>(null);

  // Accumulate remote tracks into one stable MediaStream (ontrack fires per-track)
  const remoteStreamRef = useRef<MediaStream>(new MediaStream());

  // Pending streams — arrived before video elements were in the DOM
  const pendingRemoteStream = useRef<MediaStream | null>(null);
  const pendingLocalStream = useRef<MediaStream | null>(null);

  // Keep ref in sync for stale closures
  useEffect(() => { callStateRef.current = callState; }, [callState]);

  // When status changes (e.g. ringing→connected), video elements mount.
  // Apply any streams that arrived before the DOM was ready.
  useEffect(() => {
    wlog('STATUS', `callState.status → ${callState.status}`);
    if (pendingRemoteStream.current && remoteVideoRef.current) {
      wlog('VIDEO', 'applying pending remote stream after status change');
      remoteVideoRef.current.srcObject = pendingRemoteStream.current;
      pendingRemoteStream.current = null;
    }
    if (pendingLocalStream.current && localVideoRef.current) {
      wlog('VIDEO', 'applying pending local stream after status change');
      localVideoRef.current.srcObject = pendingLocalStream.current;
      pendingLocalStream.current = null;
    }
  }, [callState.status]);

  const setRemoteVideo = useCallback((stream: MediaStream) => {
    const tracks = stream.getTracks().map(t => `${t.kind}(${t.id.slice(0,8)} ${t.readyState})`).join(', ');
    if (remoteVideoRef.current) {
      const prev = remoteVideoRef.current.srcObject;
      const changed = prev !== stream;
      wlog('VIDEO', `setRemoteVideo → srcObject ${changed ? 'CHANGED' : 'same'}, tracks: [${tracks}]`);
      remoteVideoRef.current.srcObject = stream;
    } else {
      wlog('VIDEO', `setRemoteVideo → el not mounted yet, queued. tracks: [${tracks}]`);
      pendingRemoteStream.current = stream;
    }
  }, []);

  const setLocalVideo = useCallback((stream: MediaStream) => {
    const tracks = stream.getTracks().map(t => `${t.kind}(${t.id.slice(0,8)} ${t.readyState})`).join(', ');
    if (localVideoRef.current) {
      const prev = localVideoRef.current.srcObject;
      const changed = prev !== stream;
      wlog('VIDEO', `setLocalVideo → srcObject ${changed ? 'CHANGED' : 'same'}, tracks: [${tracks}]`);
      localVideoRef.current.srcObject = stream;
    } else {
      wlog('VIDEO', `setLocalVideo → el not mounted yet, queued. tracks: [${tracks}]`);
      pendingLocalStream.current = stream;
    }
  }, []);

  const cleanup = useCallback(() => {
    wlog('CLEANUP', 'cleanup() called', { stack: new Error().stack?.split('\n').slice(1,4).join(' | ') });
    // A2 — clear watchdog
    clearTimeout(disconnectTimerRef.current ?? undefined);
    disconnectTimerRef.current = null;

    // A4 — clear ABR BEFORE closing PC to prevent race with interval callback
    if (abrRef.current.intervalId) {
      clearInterval(abrRef.current.intervalId);
    }
    abrRef.current = { level: 'high', stableSeconds: 0, intervalId: null, graceTicks: 3 };

    // ICE queue reset
    pendingCandidatesRef.current = [];
    remoteDescSetRef.current = false;

    // B2 — null the DataChannel ref; pc.close() handles SCTP shutdown
    // Do NOT call dc.close() here — it sends SCTP SHUTDOWN to peer triggering their cleanup
    dataChannelRef.current = null;
    setCallMessages([]);

    // Fix 3 — reset remote stream accumulator
    remoteStreamRef.current = new MediaStream();

    pcRef.current?.close();
    pcRef.current = null;
    localStreamRef.current?.getTracks().forEach(t => t.stop());
    localStreamRef.current = null;
    screenStreamRef.current?.getTracks().forEach(t => t.stop());
    screenStreamRef.current = null;
    pendingRemoteStream.current = null;
    pendingLocalStream.current = null;
    setCallState(INITIAL_STATE);
  }, []);

  // ICE candidate helpers
  const addCandidateSafe = useCallback(async (pc: RTCPeerConnection, candidate: RTCIceCandidateInit) => {
    if (!remoteDescSetRef.current) {
      wlog('ICE', `queued candidate (remoteDesc not set yet), queue size: ${pendingCandidatesRef.current.length + 1}`);
      pendingCandidatesRef.current.push(candidate);
      return;
    }
    wlog('ICE', 'addIceCandidate', candidate.candidate?.slice(0, 60));
    await pc.addIceCandidate(candidate).catch(e => {
      wlog('ICE', 'addIceCandidate ERROR', e);
      console.warn('[ICE] addIceCandidate:', e);
    });
  }, []);

  const drainPendingCandidates = useCallback(async (pc: RTCPeerConnection) => {
    wlog('ICE', `drainPendingCandidates: ${pendingCandidatesRef.current.length} queued`);
    remoteDescSetRef.current = true;
    for (const c of pendingCandidatesRef.current) {
      await pc.addIceCandidate(c).catch(e => {
        wlog('ICE', 'drain ERROR', e);
        console.warn('[ICE] drain:', e);
      });
    }
    pendingCandidatesRef.current = [];
  }, []);

  // A4 — applyBitrateLevel: only maxBitrate via setParameters — no applyConstraints during call
  // (applyConstraints restarts camera capture → freeze frames every transition)
  const applyBitrateLevel = useCallback(async (pc: RTCPeerConnection, level: BitrateLevel) => {
    wlog('ABR', `applyBitrateLevel → ${level}`);
    for (const sender of pc.getSenders()) {
      if (!sender.track) continue;
      const kind = sender.track.kind as 'audio' | 'video';
      const params = sender.getParameters();
      if (!params.encodings?.length) params.encodings = [{}];
      params.encodings[0].maxBitrate = BITRATE_LEVELS[level][kind];
      await sender.setParameters(params).catch(e => console.warn('[ABR] setParameters:', e));
    }
  }, []);

  // A4 — startABR: poll stats every 2s and adjust bitrate level
  const startABR = useCallback((pc: RTCPeerConnection) => {
    // Defensive: clear any leftover interval before starting a new one
    if (abrRef.current.intervalId) {
      clearInterval(abrRef.current.intervalId);
    }
    abrRef.current = { level: 'high', stableSeconds: 0, intervalId: null, graceTicks: 3 };

    // Apply initial bitrate cap NOW — after negotiation is complete, not before
    applyBitrateLevel(pc, 'high').catch(() => {});

    abrRef.current.intervalId = setInterval(async () => {
      // Guard: skip if PC is no longer connected (avoids getStats on closed PC)
      if (pc.connectionState !== 'connected') return;

      // Grace period: skip first N ticks after connected/reconnected to let stats stabilize
      if (abrRef.current.graceTicks > 0) { abrRef.current.graceTicks--; return; }

      try {
        const stats = await pc.getStats();
        let packetLoss = 0, rtt = 0;
        stats.forEach(r => {
          if (r.type === 'inbound-rtp' && r.kind === 'video') {
            const total = (r.packetsReceived ?? 0) + (r.packetsLost ?? 0);
            packetLoss = total > 0 ? (r.packetsLost ?? 0) / total : 0;
          }
          if (r.type === 'candidate-pair' && r.state === 'succeeded')
            rtt = (r.currentRoundTripTime ?? 0) * 1000;
        });

        const abr = abrRef.current;
        // Only use real network signals (packet loss, RTT) — not qualityLimitationReason
        // which fires 'bandwidth' spuriously on loopback/LAN during encoder warm-up
        const shouldDown = packetLoss > 0.05 || rtt > 300;
        const canUp = abr.stableSeconds >= 10 && packetLoss < 0.01 && rtt < 150;
        const current = abr.level;
        let next = current;

        if (shouldDown) {
          abr.stableSeconds = 0;
          next = current === 'high' ? 'medium' : 'low';
        } else if (canUp) {
          abr.stableSeconds = 0;
          next = current === 'low' ? 'medium' : 'high';
        } else {
          abr.stableSeconds += 2;
        }

        if (next !== current) {
          wlog('ABR', `level change ${current} → ${next}`, { packetLoss: packetLoss.toFixed(3), rtt: rtt.toFixed(0) });
          abr.level = next;
          await applyBitrateLevel(pc, next);
        }
      } catch {
        // PC was closed between the guard check and getStats — ignore
      }
    }, 2_000);
  }, [applyBitrateLevel]);

  // B2 — DataChannel handlers
  const attachDataChannelHandlers = useCallback((dc: RTCDataChannel) => {
    dc.onopen  = () => console.log('[DataChannel] open');
    dc.onclose = () => console.log('[DataChannel] closed');
    dc.onerror = (e) => console.warn('[DataChannel] error', e);
    dc.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as CallChatMessage;
        setCallMessages(prev => [...prev, { ...msg, fromSelf: false }]);
      } catch {
        console.warn('[DataChannel] unparseable:', event.data);
      }
    };
  }, []);

  const createPC = useCallback((peerId: string, callId: string) => {
    wlog('PC', `createPC peerId=${peerId.slice(0,8)} callId=${callId.slice(0,8)}`);
    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });

    pc.onicecandidate = ({ candidate }) => {
      if (!candidate) {
        wlog('ICE', 'gathering complete (null candidate)');
        return;
      }
      wlog('ICE', `local candidate: ${candidate.candidate.slice(0, 60)}`);
      const signal: Signal = {
        type: 'ice_candidate',
        candidate: candidate.candidate,
        sdp_mid: candidate.sdpMid,
        sdp_mline_index: candidate.sdpMLineIndex,
      };
      api.sendCallSignal(peerId, callId, JSON.stringify(signal));
    };

    pc.ontrack = (event) => {
      const t = event.track;
      wlog('TRACK', `ontrack kind=${t.kind} id=${t.id.slice(0,8)} readyState=${t.readyState} streams=${event.streams.length}`);
      t.onmute   = () => wlog('TRACK', `track MUTED   kind=${t.kind} id=${t.id.slice(0,8)}`);
      t.onunmute = () => wlog('TRACK', `track UNMUTED kind=${t.kind} id=${t.id.slice(0,8)}`);
      t.onended  = () => wlog('TRACK', `track ENDED   kind=${t.kind} id=${t.id.slice(0,8)}`);
      remoteStreamRef.current.addTrack(event.track);
      const allTracks = remoteStreamRef.current.getTracks().map(x => `${x.kind}(${x.id.slice(0,8)})`).join(', ');
      wlog('TRACK', `remoteStream now has: [${allTracks}]`);
      setRemoteVideo(remoteStreamRef.current);
    };

    // B2 — callee receives DataChannel
    pc.ondatachannel = (event) => {
      dataChannelRef.current = event.channel;
      attachDataChannelHandlers(event.channel);
    };

    // A2 — disconnected watchdog + A4 — start ABR on connected
    pc.onconnectionstatechange = () => {
      const state = pc.connectionState;
      wlog('PC', `connectionState → ${state} (signalingState=${pc.signalingState} iceGathering=${pc.iceGatheringState})`);
      console.log('[WebRTC] connectionState:', state);
      if (state === 'connected') {
        clearTimeout(disconnectTimerRef.current ?? undefined);
        disconnectTimerRef.current = null;
        // Avoid re-render storm on reconnect: don't overwrite startTime
        setCallState(prev => prev.status === 'connected'
          ? prev
          : { ...prev, status: 'connected', startTime: Date.now() }
        );
        // Only start ABR once — guard against reconnect cycles
        if (!abrRef.current.intervalId) {
          startABR(pc);
        } else {
          // Reconnect: reset grace period so stale post-ICE-restart stats are ignored
          abrRef.current.graceTicks = 3;
        }
      }
      if (state === 'disconnected') {
        // Only the offerer (caller) initiates ICE restart to avoid signaling collision.
        // Answerer waits for the restart offer from the other side.
        const isOfferer = pc.localDescription?.type === 'offer';
        console.log(`[WebRTC] disconnected — ${isOfferer ? 'attempting ICE restart' : 'waiting for peer ICE restart'}`);
        if (isOfferer) {
          try { pc.restartIce(); } catch { /* watchdog will handle it */ }
        }
        // Clear any existing timer before setting a new one (handles rapid disconnect cycles)
        clearTimeout(disconnectTimerRef.current ?? undefined);
        disconnectTimerRef.current = setTimeout(() => {
          if (pcRef.current?.connectionState !== 'connected') {
            console.log('[WebRTC] recovery timeout, cleaning up');
            cleanup();
          }
        }, 15_000);
      }
      if (state === 'failed') {
        clearTimeout(disconnectTimerRef.current ?? undefined);
        disconnectTimerRef.current = null;
        cleanup();
      }
    };

    pc.onicegatheringstatechange = () => {
      wlog('ICE', `gatheringState → ${pc.iceGatheringState}`);
    };

    pc.onsignalingstatechange = () => {
      wlog('SDP', `signalingState → ${pc.signalingState}`);
    };

    pc.oniceconnectionstatechange = () => {
      const iceState = pc.iceConnectionState;
      wlog('ICE', `iceConnectionState → ${iceState}`);
      console.log('[ICE] iceConnectionState:', iceState);
      if (iceState === 'failed') {
        console.warn('[ICE] ICE failed — attempting restart');
        try {
          pc.restartIce();
        } catch {
          cleanup();
        }
      }
    };

    pc.onnegotiationneeded = async () => {
      wlog('SDP', `negotiationneeded (signalingState=${pc.signalingState})`);
      // Skip if signaling is in progress
      if (pcRef.current?.signalingState !== 'stable') {
        wlog('SDP', 'negotiationneeded SKIPPED — not stable');
        return;
      }
      // Skip during initial call setup: startCall/answerCall manage their own SDP exchange.
      // localDescription is null until setLocalDescription is called for the first time.
      if (pc.localDescription === null) {
        wlog('SDP', 'negotiationneeded SKIPPED — initial negotiation not done yet');
        return;
      }
      const { peerId, callId, callType } = callStateRef.current;
      if (!peerId || !callId) {
        wlog('SDP', 'negotiationneeded SKIPPED — no peerId/callId');
        return;
      }
      wlog('SDP', 'negotiationneeded → renegotiating');
      console.log('[WebRTC] negotiationneeded — renegotiating');
      try {
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        const signal: Signal = { type: 'offer', sdp: offer.sdp!, call_type: callType };
        await api.sendCallSignal(peerId, callId, JSON.stringify(signal));
      } catch (e) {
        console.error('[WebRTC] renegotiation failed:', e);
      }
    };

    pcRef.current = pc;
    return pc;
  }, [setRemoteVideo, cleanup, startABR, attachDataChannelHandlers, addCandidateSafe, drainPendingCandidates]);

  const startCall = useCallback(async (peerId: string, peerName: string | null, callType: CallType) => {
    const callId = crypto.randomUUID();
    wlog('CALL', `startCall peerId=${peerId.slice(0,8)} callType=${callType} callId=${callId.slice(0,8)}`);

    setCallState({ ...INITIAL_STATE, status: 'calling', peerId, peerName, callId, callType });

    try {
      const pc = createPC(peerId, callId);

      // B2 — caller creates DataChannel BEFORE createOffer
      const dc = pc.createDataChannel('chat', { ordered: true });
      dataChannelRef.current = dc;
      attachDataChannelHandlers(dc);

      // A1 — constrained getUserMedia
      wlog('MEDIA', `getUserMedia callType=${callType}`);
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: AUDIO_CONSTRAINTS,
        video: callType === 'video' ? VIDEO_CONSTRAINTS : false,
      });
      const tracks = stream.getTracks().map(t => `${t.kind}(${t.label.slice(0,20)} ${t.id.slice(0,8)})`).join(', ');
      wlog('MEDIA', `getUserMedia OK tracks: [${tracks}]`);
      localStreamRef.current = stream;
      stream.getTracks().forEach(t => { wlog('PC', `addTrack ${t.kind}`); pc.addTrack(t, stream); });
      setLocalVideo(stream);

      wlog('SDP', 'createOffer');
      const offer = await pc.createOffer();
      wlog('SDP', 'setLocalDescription(offer)');
      await pc.setLocalDescription(offer);

      wlog('SDP', 'sendCallSignal offer');
      const signal: Signal = { type: 'offer', sdp: offer.sdp!, call_type: callType };
      await api.sendCallSignal(peerId, callId, JSON.stringify(signal));
      wlog('SDP', 'offer sent');
    } catch (err) {
      wlog('CALL', 'startCall ERROR', err);
      console.error('startCall failed:', err);
      alert('Failed to start call: ' + err);
      cleanup();
    }
  }, [createPC, cleanup, setLocalVideo, attachDataChannelHandlers]);

  const answerCall = useCallback(async (peerId: string, callId: string, offerSdp: string, callType: CallType) => {
    wlog('CALL', `answerCall peerId=${peerId.slice(0,8)} callType=${callType} callId=${callId.slice(0,8)}`);
    try {
      const pc = createPC(peerId, callId);

      // A1 — constrained getUserMedia
      wlog('MEDIA', `getUserMedia callType=${callType}`);
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: AUDIO_CONSTRAINTS,
        video: callType === 'video' ? VIDEO_CONSTRAINTS : false,
      });
      const tracks = stream.getTracks().map(t => `${t.kind}(${t.label.slice(0,20)} ${t.id.slice(0,8)})`).join(', ');
      wlog('MEDIA', `getUserMedia OK tracks: [${tracks}]`);
      localStreamRef.current = stream;
      stream.getTracks().forEach(t => { wlog('PC', `addTrack ${t.kind}`); pc.addTrack(t, stream); });
      setLocalVideo(stream);

      wlog('SDP', 'setRemoteDescription(offer)');
      await pc.setRemoteDescription({ type: 'offer', sdp: offerSdp });
      await drainPendingCandidates(pc);
      wlog('SDP', 'createAnswer');
      const answer = await pc.createAnswer();
      wlog('SDP', 'setLocalDescription(answer)');
      await pc.setLocalDescription(answer);

      wlog('SDP', 'sendCallSignal answer');
      const signal: Signal = { type: 'answer', sdp: answer.sdp! };
      await api.sendCallSignal(peerId, callId, JSON.stringify(signal));
      wlog('SDP', 'answer sent');
      // Status transitions to 'connected' via onconnectionstatechange only — no duplicate here
    } catch (err) {
      wlog('CALL', 'answerCall ERROR', err);
      console.error('answerCall failed:', err);
      alert('Failed to answer call: ' + err);
      cleanup();
    }
  }, [createPC, cleanup, setLocalVideo, drainPendingCandidates]);

  const rejectCall = useCallback(async (peerId: string, callId: string) => {
    const signal: Signal = { type: 'reject' };
    await api.sendCallSignal(peerId, callId, JSON.stringify(signal));
    cleanup();
  }, [cleanup]);

  const hangup = useCallback(async () => {
    const { peerId, callId } = callStateRef.current;
    if (peerId && callId) {
      const signal: Signal = { type: 'hangup' };
      await api.sendCallSignal(peerId, callId, JSON.stringify(signal)).catch(() => {});
    }
    cleanup();
  }, [cleanup]);

  const toggleMute = useCallback(() => {
    localStreamRef.current?.getAudioTracks().forEach(t => { t.enabled = !t.enabled; });
    setCallState(prev => ({ ...prev, isMuted: !prev.isMuted }));
  }, []);

  const toggleVideo = useCallback(() => {
    localStreamRef.current?.getVideoTracks().forEach(t => { t.enabled = !t.enabled; });
    setCallState(prev => ({ ...prev, isVideoOff: !prev.isVideoOff }));
  }, []);

  const toggleScreenShare = useCallback(async () => {
    if (callStateRef.current.isSharingScreen) {
      screenStreamRef.current?.getTracks().forEach(t => t.stop());
      screenStreamRef.current = null;
      const camTrack = localStreamRef.current?.getVideoTracks()[0];
      if (camTrack && pcRef.current) {
        const sender = pcRef.current.getSenders().find(s => s.track?.kind === 'video');
        if (sender) await sender.replaceTrack(camTrack);
      }
      setCallState(prev => ({ ...prev, isSharingScreen: false }));
    } else {
      const screen = await navigator.mediaDevices.getDisplayMedia({ video: true });
      screenStreamRef.current = screen;
      const screenTrack = screen.getVideoTracks()[0];
      if (pcRef.current) {
        const sender = pcRef.current.getSenders().find(s => s.track?.kind === 'video');
        if (sender) await sender.replaceTrack(screenTrack);
      }
      if (localVideoRef.current) {
        const combined = new MediaStream([
          screenTrack,
          ...(localStreamRef.current?.getAudioTracks() ?? []),
        ]);
        localVideoRef.current.srcObject = combined;
      }
      screenTrack.onended = () => toggleScreenShare();
      setCallState(prev => ({ ...prev, isSharingScreen: true }));
    }
  }, []);

  // B2 — sendCallMessage
  const sendCallMessage = useCallback((text: string) => {
    const dc = dataChannelRef.current;
    if (!dc || dc.readyState !== 'open') return;
    const msg: CallChatMessage = {
      id: crypto.randomUUID(), text: text.trim(),
      timestamp: Date.now(), fromSelf: true,
    };
    dc.send(JSON.stringify(msg));
    setCallMessages(prev => [...prev, msg]);
  }, []);

  // A3 — visibilitychange handler: throttle video on background to prevent WKWebKit freeze
  useEffect(() => {
    const handleVisibility = async () => {
      const videoTrack = localStreamRef.current?.getVideoTracks()[0];
      if (!videoTrack) return;
      if (document.visibilityState === 'hidden') {
        await videoTrack.applyConstraints({ frameRate: { max: 5 } }).catch(() => {});
      } else {
        // Restore constraints matching current ABR level, not full VIDEO_CONSTRAINTS
        const level = abrRef.current.level;
        await videoTrack.applyConstraints(RESOLUTION_LEVELS[level]).catch(() => {});
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);
    return () => document.removeEventListener('visibilitychange', handleVisibility);
  }, []);

  // Listen for incoming call signals from Rust backend
  useEffect(() => {
    const unlisten = listen<CallSignalPayload>('call-signal', async (event) => {
      const { peer_id, call_id, signal_json } = event.payload;
      const signal: Signal = JSON.parse(signal_json);
      wlog('SIGNAL', `received type=${signal.type} from=${peer_id.slice(0,8)} call=${call_id.slice(0,8)}`);

      if (signal.type === 'offer') {
        if (pcRef.current && pcRef.current.signalingState !== 'closed') {
          // Renegotiation offer from peer (e.g. screen share)
          wlog('SDP', `renegotiation offer (signalingState=${pcRef.current.signalingState})`);
          console.log('[WebRTC] renegotiation offer received');
          await pcRef.current.setRemoteDescription({ type: 'offer', sdp: signal.sdp });
          await drainPendingCandidates(pcRef.current);
          const answer = await pcRef.current.createAnswer();
          await pcRef.current.setLocalDescription(answer);
          const answerSignal: Signal = { type: 'answer', sdp: answer.sdp! };
          const { peerId, callId } = callStateRef.current;
          if (peerId && callId) await api.sendCallSignal(peerId, callId, JSON.stringify(answerSignal));
          wlog('SDP', 'renegotiation answer sent');
          return; // Don't set call state to ringing
        }
        wlog('CALL', `incoming call from ${peer_id.slice(0,8)} type=${signal.call_type}`);
        const incomingState: CallState = {
          ...INITIAL_STATE, status: 'ringing',
          peerId: peer_id, callId: call_id, callType: signal.call_type,
          peerName: null, offerSdp: signal.sdp,
        };
        setCallState(incomingState);
        onIncomingCall?.(incomingState);
      } else if (signal.type === 'answer' && pcRef.current) {
        // Guard: ignore stale answers that arrive after signaling collision resolution
        if (pcRef.current.signalingState !== 'have-local-offer') {
          console.warn('[WebRTC] ignoring stale answer in state:', pcRef.current.signalingState);
          return;
        }
        wlog('SDP', 'setRemoteDescription(answer)');
        await pcRef.current.setRemoteDescription({ type: 'answer', sdp: signal.sdp });
        await drainPendingCandidates(pcRef.current);
        wlog('SDP', 'remote answer applied');
        // Status transitions to 'connected' via onconnectionstatechange
      } else if (signal.type === 'ice_candidate' && pcRef.current) {
        await addCandidateSafe(pcRef.current, {
          candidate: signal.candidate,
          sdpMid: signal.sdp_mid,
          sdpMLineIndex: signal.sdp_mline_index,
        });
      } else if (signal.type === 'hangup' || signal.type === 'reject') {
        wlog('CALL', `received ${signal.type} from peer`);
        cleanup();
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, [cleanup, onIncomingCall, addCandidateSafe, drainPendingCandidates]);

  return {
    callState, remoteVideoRef, localVideoRef,
    startCall, answerCall, rejectCall, hangup,
    toggleMute, toggleVideo, toggleScreenShare,
    callMessages, sendCallMessage,
  };
}
