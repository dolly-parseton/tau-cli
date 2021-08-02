use std::{
    error::Error,
    fs,
    io::{self, prelude::*, stderr, stdin, stdout, BufRead, Stdin, Stdout},
    path::PathBuf,
};
use structopt::StructOpt;
use tau_engine::Rule;

type ValidatedRules = Vec<(Option<Rule>, String)>;

#[derive(StructOpt)]
#[structopt(
    name = "tau-cli",
    about = "A CLI for matching rules against JSON using the Tau Engine."
)]
struct Opt {
    /// Glob matching one or more Rule files. Rules must be '.yml' files.
    #[structopt(short, long, parse(from_os_str))]
    rules: Vec<PathBuf>,

    /// Glob matching one or more files, to be used as the input files.
    #[structopt(short, long, parse(from_os_str))]
    input: Option<Vec<PathBuf>>,

    /// Overwrite the output files.
    #[structopt(short = "f", long)]
    overwrite: bool,

    /// Overwrite the output files.
    #[structopt(short, long)]
    validate: bool,

    /// Path to write all matches, if path points to a directory then matches are written to files named after the associated rules.
    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    #[structopt(skip)]
    inner_input: Option<Input>,
    #[structopt(skip)]
    inner_output: Option<Output>,
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

enum Output {
    CommandLine(Stdout),
    Files(Vec<(fs::File, String)>),
}

impl Opt {
    pub fn validate_rules(mut self) -> Result<(Self, ValidatedRules), String> {
        //
        let mut validated_rules = Vec::new();
        for path in self.rules.iter() {
            let rule = match Rule::load(
                &fs::read_to_string(&path)
                    .map_err(|_| format!("Unable to read data from {}.", path.display()))?,
            ) {
                Ok(r) => match r.validate() {
                    Ok(true) => Some(r),
                    _ => None,
                },
                Err(_) => None,
            };
            match path.as_path().file_name().map(|f| f.to_str()).flatten() {
                Some(f) => validated_rules.push((rule, f.to_string())),
                None => return Err(format!("Unable to validate {} as a rule", path.display())),
            }
            // if rule
            //     .validate()
            //     .map_err(|_| format!("Unable to validate {} as a rule", path.display()))?
            // {
            //     match path
            //         .as_path()
            //         .file_name()
            //         .map(|f| {
            //             f.to_str()
            //                 .map(|n| n.strip_suffix(".yml").map(|s| format!("{}.match", s)))
            //         })
            //         .flatten()
            //         .flatten()
            //     {
            //         Some(f) => validated_rules.push((rule, f)),
            //         None => return Err(format!("Unable to validate {} as a rule", path.display())),
            //     }
            // }
        }
        //
        self.inner_input = Some(match self.input {
            Some(ref mut v) => match v.pop() {
                Some(p) => {
                    let f = fs::File::open(&p)
                        .map_err(|_e| format!("Unable to read input file at {}.", p.display()))?;
                    Input::Files {
                        paths: v.clone(),
                        buffer: io::BufReader::new(f),
                    }
                }
                None => {
                    return Err(
                        "No rule files provided, use -r or --rules to specify one or more rules"
                            .into(),
                    )
                }
            },
            None => Input::CommandLine(stdin()),
        });
        //
        self.inner_output = Some(match &self.output {
            Some(p) => match p.is_dir() {
                false => Output::Files(vec![(
                    fs::OpenOptions::new()
                        .write(true)
                        // Flags here ensure we're overwriting data not appending, this might tamper with match results
                        .create_new(!self.overwrite)
                        .create(self.overwrite)
                        .truncate(self.overwrite)
                        .open(&p)
                        .map_err(|_| format!("Could not create output file at {}", p.display()))?,
                    "".into(),
                )]),
                true => {
                    let mut files = Output::Files(Vec::new());
                    for (_, filename) in validated_rules.iter() {
                        if let Output::Files(ref mut v) = files {
                            v.push(
                                (fs::OpenOptions::new()
                                    .write(true)
                                    // Flags here ensure we're overwriting data not appending, this might tamper with match results
                                    .create_new(!self.overwrite)
                                    .create(self.overwrite)
                                    .truncate(self.overwrite)
                                    .open(p.join(filename))
                                    .map_err(|e| match e.kind() {
                                        io::ErrorKind::AlreadyExists => {
                                            format!("{} already exists, either remove this file or re-run with the -f / --overwrite flag ", p.join(filename).display())
                                        },
                                        io::ErrorKind::NotFound => {
                                            format!("Part of the path to {} does not exist", p.join(filename).display())
                                        }
                                        _ => format!("{:?}", e.kind()),
                                    })?,filename.into())
                            );
                        }
                    }
                    files
                }
            },
            None => Output::CommandLine(stdout()),
        });
        //
        match validated_rules.is_empty() {
            true => Err(format!(
                "Could not validate any of the following rules: {:?}",
                self.rules
            )),
            false => Ok((self, validated_rules)),
        }
    }
    pub fn output_match(
        &mut self,
        json: &serde_json::Value,
        rule_filename: &str,
    ) -> Result<(), Option<io::Error>> {
        match self.inner_output.as_mut() {
            Some(Output::Files(o)) => {
                let len = o.len();
                for (file, filename) in o.iter_mut() {
                    if filename == rule_filename || len == 1 {
                        writeln!(file, "{}", json.to_string()).map_err(Some)?;
                    }
                }
                Ok(())
            }
            Some(Output::CommandLine(ref mut stdout)) => {
                writeln!(stdout, "{}", json.to_string()).map_err(Some)?;
                Ok(())
            }
            None => Err(None),
        }
    }
}

impl Iterator for Opt {
    type Item = Result<serde_json::Value, Box<dyn Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner_input
            .as_mut()
            .map(|ref mut i| i.next())
            .flatten()
    }
}

fn main() -> Result<(), io::Error> {
    let (mut stdout, mut stderr) = (stdout(), stderr());
    let (mut opt, rules) = match Opt::from_args().validate_rules() {
        Ok(x) => x,
        Err(e) => {
            writeln!(stderr, "{}", e)?;
            std::process::exit(1);
        }
    };
    if opt.validate {
        writeln!(stdout, "Rule Name, Is Valid")?;
        for (r, n) in rules.iter() {
            writeln!(stdout, "{}, {}", n, r.is_some())?;
        }
        std::process::exit(0);
    }
    while let Some(res) = opt.next() {
        match res {
            Ok(json) => {
                for (rule, path) in rules.iter() {
                    if let Some(r) = rule {
                        if r.matches(&json) {
                            if let Err(Some(e)) = opt.output_match(&json, &path) {
                                writeln!(stderr, "An error occured whilst outputting data, {}", e)?;
                                std::process::exit(1);
                            }
                        }
                    }
                }
            }
            Err(e) => writeln!(stderr, "{}", e)?,
        }
    }
    Ok(())
}
