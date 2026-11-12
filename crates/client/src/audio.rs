//! Voice memo playback with rodio on its own thread (the output stream isn't `Send`).
//!
//! Playing natively instead of through `<audio>` means the WebView needs no AAC/ALAC
//! support, and the mock's WAV and real memos decode the same way.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Snapshot {
    pub id: Option<String>,
    pub title: String,
    pub duration: f64,
    pub position: f64,
    pub paused: bool,
    pub ended: bool,
    pub error: Option<String>,
}

enum Cmd {
    Play { id: String, title: String, duration: f64, bytes: Arc<Vec<u8>> },
    Toggle,
    Seek(f64),
    Stop,
}

pub struct Audio {
    tx: Mutex<mpsc::Sender<Cmd>>,
    state: Arc<Mutex<Snapshot>>,
}

impl Audio {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let shared = state.clone();
        std::thread::Builder::new().name("audio".into()).spawn(move || run(rx, shared)).expect("audio thread");
        Self { tx: Mutex::new(tx), state }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }

    pub fn play(&self, id: String, title: String, duration: f64, bytes: Arc<Vec<u8>>) {
        self.send(Cmd::Play { id, title, duration, bytes });
    }

    pub fn toggle(&self) {
        self.send(Cmd::Toggle);
    }

    pub fn seek(&self, secs: f64) {
        self.send(Cmd::Seek(secs));
    }

    pub fn stop(&self) {
        self.send(Cmd::Stop);
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.lock().unwrap().send(cmd);
    }
}

struct Current {
    bytes: Arc<Vec<u8>>,
    player: rodio::Player,
}

fn run(rx: mpsc::Receiver<Cmd>, state: Arc<Mutex<Snapshot>>) {
    let mut sink: Option<rodio::MixerDeviceSink> = None;
    let mut current: Option<Current> = None;

    let start = |sink: &mut Option<rodio::MixerDeviceSink>, bytes: &Arc<Vec<u8>>| -> Result<rodio::Player, String> {
        if sink.is_none() {
            *sink = Some(rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("No audio output: {e}"))?);
        }
        let source = rodio::Decoder::try_from(std::io::Cursor::new(bytes.as_ref().clone())).map_err(|e| format!("Can't play this recording: {e}"))?;
        let player = rodio::Player::connect_new(sink.as_ref().unwrap().mixer());
        player.append(source);
        Ok(player)
    };

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Cmd::Play { id, title, duration, bytes }) => {
                if let Some(old) = current.take() {
                    old.player.stop();
                }
                let mut s = state.lock().unwrap();
                *s = Snapshot { id: Some(id), title, duration, ..Default::default() };
                match start(&mut sink, &bytes) {
                    Ok(player) => current = Some(Current { bytes, player }),
                    Err(err) => {
                        s.id = None;
                        s.error = Some(err);
                    }
                }
            }
            Ok(Cmd::Toggle) => {
                let ended = state.lock().unwrap().ended;
                if let Some(cur) = &mut current {
                    if ended {
                        // Replay from the start.
                        if let Ok(player) = start(&mut sink, &cur.bytes) {
                            cur.player = player;
                            state.lock().unwrap().ended = false;
                        }
                    } else if cur.player.is_paused() {
                        cur.player.play();
                    } else {
                        cur.player.pause();
                    }
                }
            }
            Ok(Cmd::Seek(secs)) => {
                if let Some(cur) = &mut current {
                    if state.lock().unwrap().ended
                        && let Ok(player) = start(&mut sink, &cur.bytes)
                    {
                        player.pause();
                        cur.player = player;
                        state.lock().unwrap().ended = false;
                    }
                    let _ = cur.player.try_seek(Duration::from_secs_f64(secs.max(0.0)));
                }
            }
            Ok(Cmd::Stop) => {
                if let Some(old) = current.take() {
                    old.player.stop();
                }
                *state.lock().unwrap() = Snapshot::default();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some(cur) = &current {
            let mut s = state.lock().unwrap();
            if cur.player.empty() {
                s.ended = true;
                s.paused = true;
                s.position = s.duration;
            } else {
                s.paused = cur.player.is_paused();
                s.position = cur.player.get_pos().as_secs_f64();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The mock server serves 16-bit PCM WAV; real memos are AAC/ALAC in MP4. All must decode.
    #[test]
    fn decodes_pcm_wav() {
        let rate = 16_000u32;
        let samples = rate / 4;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        wav.extend(std::iter::repeat_n(0u8, samples as usize * 2));
        let decoder = rodio::Decoder::try_from(std::io::Cursor::new(wav)).expect("wav decodes");
        assert!(decoder.count() >= samples as usize);
    }
}
