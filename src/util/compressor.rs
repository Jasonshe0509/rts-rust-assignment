use std::io::{Read, Write};
use flate2::{write::ZlibEncoder, Compression};
use flate2::read::ZlibDecoder;
use serde::de::DeserializeOwned;
use serde::Serialize;
pub struct Compressor;

impl Compressor {
    pub fn compress<T: Serialize>(data: &T) -> Vec<u8> {
        let serialized = serde_json::to_vec(data).unwrap();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&serialized).unwrap();
        encoder.finish().unwrap()
    }

    pub fn decompress<T: DeserializeOwned>(data: &Vec<u8>) -> T {
        let mut decoder = ZlibDecoder::new(&data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        serde_json::from_slice(&decompressed).unwrap()
    }
}