use log::{error, info, warn};
use serde_json::from_str;
use std::{
    error::Error,
    fs,
    io::{self, stdin, BufRead, Stdin},
    path::PathBuf,
};
use structopt::StructOpt;
use tau_engine::Rule;

#[derive(StructOpt)]
#[structopt(
    name = "tau-cli",
    about = "A CLI for matching rules against JSON using the Tau Engine."
)]
struct Opt {
    /// Glob matching one or more Rule files
    #[structopt(short, long, parse(from_os_str))]
    rules: Vec<PathBuf>,

    /// Glob matching one or more Rule files, to be used as the input files.
    #[structopt(short, long, parse(from_os_str))]
    input: Option<Vec<PathBuf>>,

    /// Path to write all matches, if path points to a directory then matches are written to files named after the associated rules.
    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    #[structopt(skip)]
    inner: Option<Input>,
}

enum Input {
    CommandLine(Stdin),
    Files {
        paths: Vec<PathBuf>,
        buffer: io::BufReader<fs::File>,
    },
}

impl Iterator for Input {
    type Item = Result<serde_json::Value, Box<dyn Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Input::CommandLine(stdin) => match stdin.lock().lines().next() {
                Some(Ok(l)) => Some(serde_json::from_str(&l).map_err(|e| e.into())),
                Some(Err(e)) => Some(Err(e.into())),
                None => None,
            },
            Input::Files {
                ref mut paths,
                ref mut buffer,
            } => {
                // Try read from buffer
                let mut line = String::new();
                match buffer.read_line(&mut line) {
                    Err(_) | Ok(0) => {
                        match paths.pop() {
                            Some(p) => {
                                // Create a BufReader
                                match fs::OpenOptions::new().read(true).open(p) {
                                    Ok(f) => *buffer = io::BufReader::new(f),
                                    Err(e) => return Some(Err(e.into())),
                                }
                                self.next()
                            }
                            None => None,
                        }
                    }
                    Ok(_) => Some(serde_json::from_str(line.trim_end()).map_err(|e| e.into())),
                }
            }
        }
    }
}

impl Opt {
    pub fn validate_rules(mut self) -> Result<(Self, Vec<(Rule, String)>), ()> {
        //
        let mut validated_rules = Vec::new();
        for path in self.rules.drain(..) {
            match fs::read_to_string(&path) {
                Ok(ref data) => match Rule::load(data) {
                    Ok(r) => {
                        if let Ok(true) = r.validate() {
                            match path
                                .as_path()
                                .file_name()
                                .map(|f| f.to_str().map(|s| s.to_string()))
                                .flatten()
                            {
                                Some(f) => validated_rules.push((r, f)),
                                None => return Err(()),
                            }
                        } else {
                            warn!("Unable to validate rule from {}.", path.display())
                        }
                    }
                    Err(_e) => warn!("Unable to generate rule from {}.", path.display()),
                },
                Err(_e) => warn!("Unable to read data from {}.", path.display()),
            }
        }
        //
        self.inner = Some(match self.input {
            Some(ref mut v) => match v.pop() {
                Some(p) => {
                    let f = fs::File::open(&p).map_err(|_e| {
                        warn!("Unable to validate rule from {}.", p.display());
                        ()
                    })?;
                    Input::Files {
                        paths: v.clone(),
                        buffer: io::BufReader::new(f),
                    }
                }
                None => return Err(()),
            },
            None => Input::CommandLine(stdin()),
        });
        //
        Ok((self, validated_rules))
    }
    pub fn save_match(&self, json: &serde_json::Value, rule_filename: &str) -> Result<(), ()> {
        match &self.output {
            Some(p) => match p.is_dir() {
                true => fs::write(p.join(rule_filename), json.to_string()).map_err(|_e| ()),
                false => fs::write(p, json.to_string()).map_err(|_e| ()),
            },
            None => {
                println!("{}", json.to_string());
                Ok(())
            }
        }
    }
}

impl Iterator for Opt {
    type Item = Result<serde_json::Value, Box<dyn Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().map(|ref mut i| i.next()).flatten()
    }
}

fn main() {
    let (mut opt, rules) = match Opt::from_args().validate_rules() {
        Ok(x) => x,
        Err(()) => {
            error!("Unable to read command line arguments");
            std::process::exit(0);
        }
    };
    while let Some(res) = opt.next() {
        match res {
            Ok(json) => {
                for (rule, path) in rules.iter() {
                    //
                    if rule.matches(&json) {
                        opt.save_match(&json, &path);
                    }
                }
            }
            Err(e) => error!("{}", e),
        }
    }
}
