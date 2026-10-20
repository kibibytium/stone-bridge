use std::fs;

pub fn get_mime_data(file_name: &str) -> Vec<u8> {
    fs::read(format!("test/fixtures/{}", file_name)).unwrap()
}
