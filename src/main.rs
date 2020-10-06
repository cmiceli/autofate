extern crate simple_logger;
extern crate log;
extern crate git2;
#[macro_use(defer)] extern crate scopeguard;
extern crate yaml_rust;

pub mod fate;
pub mod util;
use simple_logger::SimpleLogger;
use yaml_rust::YamlLoader;
use yaml_rust::Yaml;


fn main() {
    SimpleLogger::new().init().unwrap();
    let config = YamlLoader::load_from_str(&std::fs::read_to_string("config.yaml").expect("Failed to read config.yaml")).expect("Failed to parse YAML file");
    let config = &config[0];
    fate::main_loop(&config);
}
