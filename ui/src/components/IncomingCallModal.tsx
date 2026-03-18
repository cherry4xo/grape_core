import type { CallState, CallType } from '../types';

interface IncomingCallModalProps {
  callState: CallState;
  onAnswer: (callType: CallType) => void;
  onReject: () => void;
}

export function IncomingCallModal({ callState, onAnswer, onReject }: IncomingCallModalProps) {
  return (
    <div className="modal-overlay">
      <div className="modal incoming-call-modal">
        <div className="modal-header">Incoming Call</div>
        <p className="call-peer-name">
          {callState.peerName || callState.peerId?.slice(0, 16) + '...'}
        </p>
        <p className="call-type-label">
          {callState.callType === 'video' ? '📹 Video call' : '📞 Audio call'}
        </p>
        <div className="modal-buttons">
          <button className="reject-btn" onClick={onReject}>Decline</button>
          <button className="answer-btn" onClick={() => onAnswer(callState.callType)}>Answer</button>
        </div>
      </div>
    </div>
  );
}