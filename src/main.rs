extern crate git2;
extern crate yaml_rust;

use std::process::Command;
use yaml_rust::YamlLoader;
use yaml_rust::Yaml;

struct Repo {
    directory: String,
    repo: String,
    last_commit: String,
}

struct FateResult {
    response: String,
}

fn testing() {
    let repo = match git2::Repository::open("/home/chris/git/ffmpeg") {
            Ok(repo) => repo,
            Err(e) => panic!("failed to open: {}", e),
    };
    for repo_ref in repo.references().unwrap() {
        println!("{}", repo_ref.unwrap().name().unwrap());

    }

}

fn run_fate_test(base_repo: &str, config: &Yaml, commit_hash: git2::Oid) -> Result<FateResult, std::io::Error> {
    let new_repo_loc = format!("{}/{}", config["fate"]["tmp_directory"].as_str().unwrap(),commit_hash);
    //XXX: delete if the new repo location exists
    let new_repo = git2::Repository::clone(base_repo, &new_repo_loc).unwrap();
    new_repo.set_head_detached(commit_hash).unwrap();
    let cmd_args: Vec<&str> = config["fate"]["command"].as_str().unwrap().split(" ").collect();
    let configure = Command::new("configure").current_dir(&new_repo_loc).status().unwrap();
    let res = match Command::new(cmd_args[0])
        .args(&cmd_args[1..])
        .env("FATE_SAMPLES", config["fate"]["samples_directory"].as_str().unwrap())
        .current_dir(&new_repo_loc)
        .status() {
        Ok(status) => return Ok(FateResult{response: "Everything went ok".to_string()}),
        Err(e) => return Err(e),
    };
}

fn main() {
    //testing();
    //std::process::exit(0);
    // read config file
    let base_commit = "d1c6e09d09f530a1f103a5dbdf06f69f42611974";
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
    let base_commit = git2::Oid::from_str(base_commit).unwrap();
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
    run_fate_test(config["repo"]["directory"].as_str().unwrap(), config, unseen_commits[unseen_commits.len()-1]);
}
