use std::path::Path;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use log::{error, info, trace};
use std::process::Command;
use yaml_rust::Yaml;
use std::convert::TryInto;
use crate::util;

struct FateResult {
    report: Option<String>,
    error: Option<String>,
}

fn collect_results(base_repo: &str) -> Result<HashMap<String, FateResult>, std::io::Error> {
    let mut results: HashMap<String, FateResult> = HashMap::new();
    let report_path = format!("{}/tests/data/fate/", base_repo);
    info!("Collecting report results from {}", &report_path);
    let walker = walkdir::WalkDir::new(&report_path).contents_first(true).into_iter();
    for entry in walker.filter_entry(|e| crate::util::is_report_file(e)) {
        info!("Found file: {}", &entry.as_ref().unwrap().file_name().to_str().unwrap());
        match entry.unwrap().file_name().to_str() {
            None => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to resolve a report file to string")),
            Some(s) => results.insert(crate::util::to_test_name(s), FateResult{report: Some(std::fs::read_to_string(format!("{}{}",&report_path, s))?), error: None}),
        };
    }
    info!("Collecting error results from {}", &report_path);
    let walker = walkdir::WalkDir::new(&report_path).contents_first(true).into_iter();
    for entry in walker.filter_entry(|e| crate::util::is_err_file(e)) {
        match entry.unwrap().file_name().to_str() {
            None => return Err(Error::new(ErrorKind::UnexpectedEof, "failed to resolve a report file to string")),
            Some(s) => {
                let error_file = std::fs::read_to_string(format!("{}{}",&report_path, s))?;
                results.entry(crate::util::to_test_name(s)).or_insert(FateResult{report: None, error: None}).error = Some(error_file);
                error!("Error with test {}", util::to_test_name(s));
            },
        };
    }
    Ok(results)
}

fn submit_results(config: &Yaml, fate_results: &HashMap<String, FateResult>, commit_hash: git2::Oid) -> Result<(), std::io::Error> {
    let mut resulting_string = String::from("");
    for (test, result) in fate_results {
        trace!("============================================");
        trace!("{}:", test);
        if result.report.is_some() {
            trace!("\tReport: {}", result.report.as_ref().unwrap());
            resulting_string.push_str(&format!("{}", result.report.as_ref().unwrap()));
        }
        if result.error.is_some() {
            trace!("\tError: {}", result.error.as_ref().unwrap());
        }
        trace!("============================================");
    }
    let result_file = format!("{}/{}", config["fate"]["result_directory"].as_str().unwrap(), commit_hash);
    info!("writing report to {}", &result_file);
    std::fs::write(&result_file, resulting_string)
}

fn run_commands(run_dir: &str, config: &Yaml, commit_hash: &git2::Oid) -> Result<(), std::io::Error> {
    let cmd_args: Vec<&str> = config["fate"]["command"].as_str().unwrap().split(" ").collect();
    info!("Running pre-run command: {}", config["fate"]["pre_run_command"].as_str().unwrap());
    let prerun: Vec<&str> = config["fate"]["pre_run_command"].as_str().unwrap().split(" ").collect();
    let mut prerun_cmd = Command::new(&prerun[0]);
    if prerun.len() > 1 {
        prerun_cmd.args(&prerun[1..]);
    }
    let res = prerun_cmd.current_dir(run_dir).output()?;
    let pre_run_output = format!("{}/{}.prerun.stdout", config["fate"]["result_directory"].as_str().unwrap(), commit_hash);
    std::fs::write(pre_run_output, res.stdout)?;
    let pre_run_output = format!("{}/{}.prerun.stderr", config["fate"]["result_directory"].as_str().unwrap(), commit_hash);
    std::fs::write(pre_run_output, res.stderr)?;
    if !res.status.success() {
        return Err(Error::new(ErrorKind::Other, format!("Pre-run command exited with error code: {}", res.status.code().unwrap())));
    }
    info!("Running build/test command: {}", config["fate"]["command"].as_str().unwrap());
    let res = Command::new(cmd_args[0])
        .args(&cmd_args[1..])
        .env("FATE_SAMPLES", config["fate"]["samples_directory"].as_str().unwrap())
        .current_dir(run_dir)
        .output()
        .unwrap();
    let cmd_output = format!("{}/{}.cmd.stdout", config["fate"]["result_directory"].as_str().unwrap(), commit_hash);
    std::fs::write(cmd_output, res.stdout)?;
    let cmd_output = format!("{}/{}.cmd.stderr", config["fate"]["result_directory"].as_str().unwrap(), commit_hash);
    std::fs::write(cmd_output, res.stderr)?;
    if !res.status.success() {
        return Err(Error::new(ErrorKind::Other, format!("Build/test process exited with error code: {}", res.status.code().unwrap())));
    }
    Ok(())
}

fn run_fate_test(base_repo: &str, config: &Yaml, commit_hash: git2::Oid) -> Result<bool, std::io::Error> {
    let new_repo_loc = format!("{}/{}", config["fate"]["tmp_directory"].as_str().unwrap(),commit_hash);
    if Path::exists(Path::new(&new_repo_loc)) {
        info!("Removing existing repo {}", &new_repo_loc);
        std::fs::remove_dir_all(Path::new(&new_repo_loc))?;
    }
    info!("Cloning repo to {}", &new_repo_loc);
    let new_repo = git2::Repository::clone(base_repo, &new_repo_loc).unwrap();
    defer!{
        std::fs::remove_dir_all(Path::new(&new_repo_loc)).unwrap();
    };
    match new_repo.set_head_detached(commit_hash) {
        Ok(k) => k,
        Err(e) => return Err(Error::new(ErrorKind::UnexpectedEof, format!("Could not set the repo to the correct state: {}", e.message()))),
    }
    info!("Head has been set detatched to commit {}", &commit_hash);
    let mut deferred_error: Option<std::io::Error> = None;
    let res = run_commands(&new_repo_loc, config, &commit_hash);
    // Leaving Err for later because we may still want the fate results to this point.
    match res {
        Ok(_) => {},
        Err(e) => deferred_error = Some(e),
    };
    info!("Collecting results");
    let fate_results = collect_results(&new_repo_loc)?;
    info!("submitting results");
    submit_results(config, &fate_results, commit_hash)?;
    if deferred_error.is_some() {
        error!("{:?}", deferred_error);
        return Err(deferred_error.unwrap());
    }
    info!("Run was successful");
    Ok(true)
}

pub fn main_loop(config: &Yaml) {
    let base_commit = std::fs::read_to_string(config["repo"]["commit_file"].as_str().unwrap()).unwrap();
    // read state of git
    let repo = match git2::Repository::open(config["repo"]["directory"].as_str().unwrap()) {
        Ok(repo) => repo,
        Err(e) => panic!("failed to open: {}", e),
    };
    loop {
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
        for unseen_commit in unseen_commits.iter() {
            match run_fate_test(config["repo"]["directory"].as_str().unwrap(), config, *unseen_commit) {
                Ok(_) =>  {
                    match util::save_last_commit(&format!("{}", &unseen_commit)) {
                        Ok(_) => {},
                        Err(e) => {
                            error!("Failed to save commit {}: {}", &unseen_commit, e);
                            break;
                        },
                    }
                },
                Err(e) => panic!("Failed to test: {}", e),
            };
        }
        std::thread::sleep(std::time::Duration::from_secs(config["fate"]["run_interval_sec"].as_i64().unwrap().try_into().unwrap()));
    }
}
