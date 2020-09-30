extern crate git2;
extern crate yaml_rust;
extern crate log;
extern crate simple_logger;

use std::path::Path;
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use log::{info, warn, error};
use std::process::Command;
use yaml_rust::YamlLoader;
use yaml_rust::Yaml;

struct Repo {
    directory: String,
    repo: String,
    last_commit: String,
}

struct FateResult {
    Report: Option<String>,
    Error: Option<String>,
}

fn to_test_name(filename: &str) -> String {
    Path::new(filename).file_stem().unwrap().to_os_string().into_string().unwrap()
}

fn is_err_file(entry: &walkdir::DirEntry) -> bool{
    entry.file_name()
         .to_str()
         .map(|s| s.ends_with(".err"))
         .unwrap_or(false)
}

fn is_report_file(entry: &walkdir::DirEntry) -> bool{
    entry.file_name()
         .to_str()
         .map(|s| s.ends_with(".rep"))
         .unwrap_or(false)
}

fn collect_results(base_repo: &str) -> Result<HashMap<String, FateResult>, std::io::Error> {
    let mut results: HashMap<String, FateResult> = HashMap::new();
    let report_path = format!("{}/tests/data/fate/", base_repo);
    info!("Collecting report results from {}", &report_path);
    let walker = walkdir::WalkDir::new(&report_path).contents_first(true).into_iter();
    for entry in walker.filter_entry(|e| is_report_file(e)) {
        info!("Found file: {}", &entry.as_ref().unwrap().file_name().to_str().unwrap());
        match entry.unwrap().file_name().to_str() {
            None => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to resolve a report file to string")),
            Some(s) => results.insert(to_test_name(s), FateResult{Report: Some(std::fs::read_to_string(format!("{}{}",&report_path, s))?), Error: None}),
        };
    }
    info!("Collecting error results from {}", &report_path);
    let walker = walkdir::WalkDir::new(&report_path).contents_first(true).into_iter();
    for entry in walker.filter_entry(|e| is_err_file(e)) {
        match entry.unwrap().file_name().to_str() {
            None => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to resolve a report file to string")),
            Some(s) => {
                let error_file = std::fs::read_to_string(format!("{}{}",&report_path, s))?;
                results.entry(to_test_name(s)).or_insert(FateResult{Report: None, Error: None}).Error = Some(error_file);
                error!("Error with test {}", to_test_name(s));
            },
        };
    }
    Ok(results)
}

fn submit_results(config: &Yaml, fate_results: &HashMap<String, FateResult>, commit_hash: git2::Oid) -> Result<(), std::io::Error> {
    let mut resulting_string = String::from("");
    for (test, result) in fate_results {
        println!("============================================");
        println!("{}:", test);
        if result.Report.is_some() {
            println!("\tReport: {}", result.Report.as_ref().unwrap());
            resulting_string.push_str(&format!("{}", result.Report.as_ref().unwrap()));
        }
        if result.Error.is_some() {
            println!("\tError: {}", result.Error.as_ref().unwrap());
        }
        println!("============================================");
    }
    std::fs::write(format!("{}/{}", config["fate"]["result_directory"].as_str().unwrap(), commit_hash), resulting_string)
}

fn run_fate_test(base_repo: &str, config: &Yaml, commit_hash: git2::Oid) -> Result<bool, std::io::Error> {
    let new_repo_loc = format!("{}/{}", config["fate"]["tmp_directory"].as_str().unwrap(),commit_hash);
    if Path::exists(Path::new(&new_repo_loc)) {
        info!("Removing existing repo {}", &new_repo_loc);
        std::fs::remove_dir_all(Path::new(&new_repo_loc))?;
    }
    info!("Cloning");
    let new_repo = git2::Repository::clone(base_repo, &new_repo_loc).unwrap();
    match new_repo.set_head_detached(commit_hash) {
        Ok(k) => k,
        Err(e) => return Err(Error::new(ErrorKind::UnexpectedEof, format!("Could not set the repo to the correct state: {}", e.message()))),
    }
    info!("Head has been set detatched to commit {}", &commit_hash);
    let cmd_args: Vec<&str> = config["fate"]["command"].as_str().unwrap().split(" ").collect();
    //XXX: move to pre-run command
    info!("Running pre-run command");
    let prerun: Vec<&str> = config["fate"]["pre_run_command"].as_str().unwrap().split(" ").collect();
    let mut prerun_cmd = Command::new(&prerun[0]);
    if prerun.len() > 1 {
        prerun_cmd.args(&prerun[1..]);
    }
    prerun_cmd.current_dir(&new_repo_loc).status()?;
    let mut deferred_error: Option<Error> = None;
    info!("Running build/test command");
    let res = Command::new(cmd_args[0])
        .args(&cmd_args[1..])
        .env("FATE_SAMPLES", config["fate"]["samples_directory"].as_str().unwrap())
        .current_dir(&new_repo_loc)
        .status()
        .unwrap();
    if !res.success() {
        deferred_error = Some(Error::new(ErrorKind::Other, format!("Build/test process exited with error code: {}", res.code().unwrap())));
    }
    info!("Collecting results");
    let fate_results = collect_results(&new_repo_loc)?;
    info!("submitting results");
    submit_results(config, &fate_results, commit_hash);
    if deferred_error.is_some() {
        error!("{:?}", deferred_error);
        return Err(deferred_error.unwrap());
    }
    info!("Run was successful");
    Ok(true)
}

fn save_last_commit(commit: &str) -> Result<(), std::io::Error>{
    std::fs::write("last_commit.txt", commit)
}

fn main() {
    SimpleLogger::new().init().unwrap();
    let base_commit = std::fs::read_to_string("last_commit.txt").unwrap();
    info!("{}: {}", base_commit, base_commit.len());
    let config = YamlLoader::load_from_str(&std::fs::read_to_string("config.yaml").expect("Failed to read config.yaml")).expect("Failed to parse YAML file");
    let config = &config[0];
    // read state of git
    let repo = match git2::Repository::open(config["repo"]["directory"].as_str().unwrap()) {
            Ok(repo) => repo,
            Err(e) => panic!("failed to open: {}", e),
    };
    // update the repo with origin
    repo.find_remote("origin").unwrap().fetch(&["master"], None, None).unwrap();
    // for each commit, clone it into a repo of it's own (locally of course)
    let commit = repo.find_reference("refs/remotes/origin/master").unwrap().peel_to_commit().unwrap();
    repo.set_head_detached(commit.id()).unwrap();
    let base_commit = git2::Oid::from_str(&base_commit).unwrap();
    let mut walker= repo.revwalk().unwrap();
    walker.push(commit.id()).unwrap();
    let unseen_commits: Vec<git2::Oid>  = walker.scan((), |(), n| {
        let n = n.unwrap();
        if n != base_commit {
            return Some(n);
        }
        None
    }).fuse().collect();
    // run fate, with a ceiling of invocations
    println!("{}", unseen_commits.len());
    match run_fate_test(config["repo"]["directory"].as_str().unwrap(), config, unseen_commits[unseen_commits.len()-1]) {
        Ok(_) => save_last_commit(&format!("{}", unseen_commits[unseen_commits.len()-1])),
        Err(e) => panic!("Failed to test: {}", e),
    };
}
