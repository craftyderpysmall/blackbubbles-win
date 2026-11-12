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
        let source = decoder(bytes.as_ref().clone()).map_err(|e| format!("Can't play this recording: {e}"))?;
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

/// A decoder for a whole recording held in memory.
///
/// Says the data is seekable and how long it is. Without that (rodio's plain
/// `Decoder::try_from`), symphonia's MP4 reader stops at the first `mdat`. Voice Memos
/// writes its index (`moov`) *after* the audio, so every real recording failed with "The
/// format of the data has not been recognized" (found on real data, 2026-11-12). Knowing
/// the length also makes seeking work.
pub fn decoder(bytes: Vec<u8>) -> Result<rodio::Decoder<std::io::Cursor<Vec<u8>>>, rodio::decoder::DecoderError> {
    let len = bytes.len() as u64;
    rodio::Decoder::builder().with_data(std::io::Cursor::new(bytes)).with_byte_len(len).with_seekable(true).build()
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
        let decoder = super::decoder(wav).expect("wav decodes");
        assert!(decoder.count() >= samples as usize);
    }

    /// An MP4 box: size, type, body.
    fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// A full box: version 0 and `flags`, then the body.
    fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
        let mut b = flags.to_be_bytes().to_vec();
        b.extend_from_slice(body);
        atom(kind, &b)
    }

    fn be32(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    /// A tiny M4A laid out like Voice Memos: `ftyp`, then `mdat`, then the `moov` index.
    /// The audio is 16-bit PCM (`sowt`), so the test needs no AAC encoder.
    fn m4a_with_index_last(frames: u32) -> Vec<u8> {
        let rate = 8000u32;
        let ftyp = atom(b"ftyp", &[b"M4A ".as_slice(), &0u32.to_be_bytes(), b"M4A isom"].concat());
        let mdat = atom(b"mdat", &vec![0u8; frames as usize * 2]);
        let audio_at = (ftyp.len() + 8) as u32;
        let matrix = be32(&[0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000]);

        let mut mvhd = be32(&[0, 0, rate, frames, 0x0001_0000]);
        mvhd.extend_from_slice(&[0x01, 0x00]);
        mvhd.extend_from_slice(&[0; 10]);
        mvhd.extend_from_slice(&matrix);
        mvhd.extend_from_slice(&[0; 24]);
        mvhd.extend_from_slice(&2u32.to_be_bytes());

        let mut tkhd = be32(&[0, 0, 1, 0, frames, 0, 0]);
        tkhd.extend_from_slice(&[0, 0, 0, 0, 0x01, 0x00, 0, 0]);
        tkhd.extend_from_slice(&matrix);
        tkhd.extend_from_slice(&be32(&[0, 0]));

        let mut mdhd = be32(&[0, 0, rate, frames]);
        mdhd.extend_from_slice(&[0x55, 0xC4, 0, 0]);

        let mut hdlr = be32(&[0]);
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0; 12]);
        hdlr.push(0);

        let dinf = atom(b"dinf", &full(b"dref", 0, &[be32(&[1]), full(b"url ", 1, &[])].concat()));

        let mut sowt = vec![0u8; 6];
        sowt.extend_from_slice(&1u16.to_be_bytes()); // data reference
        sowt.extend_from_slice(&[0; 8]); // version 0, revision, vendor
        sowt.extend_from_slice(&1u16.to_be_bytes()); // mono
        sowt.extend_from_slice(&16u16.to_be_bytes()); // bits per sample
        sowt.extend_from_slice(&[0; 4]); // compression id, packet size
        sowt.extend_from_slice(&(rate << 16).to_be_bytes());
        let stbl = atom(
            b"stbl",
            &[
                full(b"stsd", 0, &[be32(&[1]), atom(b"sowt", &sowt)].concat()),
                full(b"stts", 0, &be32(&[1, frames, 1])),
                full(b"stsc", 0, &be32(&[1, 1, frames, 1])),
                full(b"stsz", 0, &be32(&[2, frames])),
                full(b"stco", 0, &be32(&[1, audio_at])),
            ]
            .concat(),
        );
        let minf = atom(b"minf", &[full(b"smhd", 0, &[0; 4]), dinf, stbl].concat());
        let mdia = atom(b"mdia", &[full(b"mdhd", 0, &mdhd), full(b"hdlr", 0, &hdlr), minf].concat());
        let trak = atom(b"trak", &[full(b"tkhd", 3, &tkhd), mdia].concat());
        let moov = atom(b"moov", &[full(b"mvhd", 0, &mvhd), trak].concat());
        [ftyp, mdat, moov].concat()
    }

    /// Real Voice Memos recordings put `moov` after `mdat`. rodio's plain `try_from`
    /// can't read that; `decoder()` must.
    #[test]
    fn decodes_m4a_with_index_after_audio() {
        let file = m4a_with_index_last(800);
        assert!(
            rodio::Decoder::try_from(std::io::Cursor::new(file.clone())).is_err(),
            "the synthetic file no longer reproduces the unseekable-source failure"
        );
        let decoder = super::decoder(file).expect("m4a with the index last decodes");
        assert_eq!(decoder.count(), 800);
    }
}
