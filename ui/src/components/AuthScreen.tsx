import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface AuthScreenProps {
  onAuthenticated: () => void;
}

type Step = 'welcome' | 'create-show' | 'import';

export function AuthScreen({ onAuthenticated }: AuthScreenProps) {
  const [step, setStep] = useState<Step>('welcome');
  const [mnemonic, setMnemonic] = useState('');
  const [importWords, setImportWords] = useState('');
  const [password, setPassword] = useState('');
  const [usePassword, setUsePassword] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const handleGenerate = async () => {
    setLoading(true);
    setError('');
    try {
      const words = await invoke<string>('auth_generate_mnemonic');
      setMnemonic(words);
      setStep('create-show');
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleSaveNew = async () => {
    if (!saved) {
      setError('Please confirm you have saved your seed phrase');
      return;
    }
    setLoading(true);
    setError('');
    try {
      await invoke('auth_save_seed', {
        words: mnemonic,
        password: usePassword && password ? password : null,
      });
      onAuthenticated();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleImport = async () => {
    setLoading(true);
    setError('');
    try {
      const valid = await invoke<boolean>('auth_validate_mnemonic', { words: importWords.trim() });
      if (!valid) {
        setError('Invalid seed phrase. Please check your 12 words.');
        setLoading(false);
        return;
      }
      await invoke('auth_save_seed', {
        words: importWords.trim(),
        password: usePassword && password ? password : null,
      });
      onAuthenticated();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const words = mnemonic.split(' ');

  if (step === 'welcome') {
    return (
      <div className="auth-screen">
        <div className="auth-card">
          <h1>VideoCalls</h1>
          <p className="auth-subtitle">Decentralized P2P Messenger</p>
          <div className="auth-buttons">
            <button onClick={handleGenerate} disabled={loading}>
              {loading ? 'Generating...' : 'Create New Account'}
            </button>
            <button className="button-secondary" onClick={() => setStep('import')}>
              Import Seed Phrase
            </button>
          </div>
          {error && <p className="auth-error">{error}</p>}
        </div>
      </div>
    );
  }

  if (step === 'create-show') {
    return (
      <div className="auth-screen">
        <div className="auth-card">
          <h2>Your Seed Phrase</h2>
          <p className="auth-subtitle">
            Write down these 12 words in order and store them safely.
            This is the only way to recover your account.
          </p>
          <div className="mnemonic-grid">
            {words.map((word, i) => (
              <div key={i} className="mnemonic-word">
                <span className="word-index">{i + 1}</span>
                <span className="word-text">{word}</span>
              </div>
            ))}
          </div>
          <div className="auth-option">
            <label>
              <input
                type="checkbox"
                checked={usePassword}
                onChange={(e) => setUsePassword(e.target.checked)}
              />
              Protect with password (optional)
            </label>
          </div>
          {usePassword && (
            <input
              type="password"
              placeholder="Enter password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="auth-input"
            />
          )}
          <label className="auth-confirm">
            <input
              type="checkbox"
              checked={saved}
              onChange={(e) => setSaved(e.target.checked)}
            />
            I have written down my seed phrase
          </label>
          <button onClick={handleSaveNew} disabled={loading || !saved}>
            {loading ? 'Saving...' : 'Continue'}
          </button>
          {error && <p className="auth-error">{error}</p>}
        </div>
      </div>
    );
  }

  if (step === 'import') {
    return (
      <div className="auth-screen">
        <div className="auth-card">
          <h2>Import Seed Phrase</h2>
          <p className="auth-subtitle">Enter your 12 words separated by spaces.</p>
          <textarea
            className="auth-textarea"
            placeholder="word1 word2 word3 ..."
            value={importWords}
            onChange={(e) => setImportWords(e.target.value)}
            rows={3}
          />
          <div className="auth-option">
            <label>
              <input
                type="checkbox"
                checked={usePassword}
                onChange={(e) => setUsePassword(e.target.checked)}
              />
              Protect with password (optional)
            </label>
          </div>
          {usePassword && (
            <input
              type="password"
              placeholder="Enter password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="auth-input"
            />
          )}
          <div className="auth-buttons">
            <button onClick={handleImport} disabled={loading}>
              {loading ? 'Importing...' : 'Import'}
            </button>
            <button className="button-secondary" onClick={() => setStep('welcome')}>
              Back
            </button>
          </div>
          {error && <p className="auth-error">{error}</p>}
        </div>
      </div>
    );
  }

  return null;
}
