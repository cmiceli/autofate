extern crate walkdir;

use std::path::Path;

pub fn to_test_name(filename: &str) -> String {
    Path::new(filename).file_stem().unwrap().to_os_string().into_string().unwrap()
}

pub fn is_err_file(entry: &walkdir::DirEntry) -> bool{
    entry.file_name()
         .to_str()
         .map(|s| s.ends_with(".err"))
         .unwrap_or(false)
}

pub fn is_report_file(entry: &walkdir::DirEntry) -> bool{
    entry.file_name()
         .to_str()
         .map(|s| s.ends_with(".rep"))
         .unwrap_or(false)
}

pub fn save_last_commit(commit: &str) -> Result<(), std::io::Error>{
    std::fs::write("last_commit.txt", commit)
}
