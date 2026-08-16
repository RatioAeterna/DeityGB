use deitygb::host_audio::SimpleAudio;
use std::convert::TryInto;
use std::io::{self, Read, Write};

fn main() {
    let (_audio, sender, sample_rate) = match SimpleAudio::new() {
        Ok(audio) => audio,
        Err(error) => {
            eprintln!("audio helper: {error}");
            std::process::exit(1);
        }
    };

    println!("{sample_rate}");
    let _ = io::stdout().flush();

    let mut input = io::stdin().lock();
    let mut frame = [0u8; 8];
    while input.read_exact(&mut frame).is_ok() {
        let left = f32::from_le_bytes(frame[0..4].try_into().unwrap());
        let right = f32::from_le_bytes(frame[4..8].try_into().unwrap());
        if sender.try_send((left, right)).is_err() && sender.send((left, right)).is_err() {
            break;
        }
    }
}
