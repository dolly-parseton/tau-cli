# tau-cli
A CLI for the `tau-engine`.

## Usage
```
tau-cli 0.1.0
A CLI for matching rules against JSON using the Tau Engine.

USAGE:
    tau-cli [FLAGS] [OPTIONS]

FLAGS:
    -h, --help         Prints help information
    -f, --overwrite    Overwrite the output files
    -V, --version      Prints version information

OPTIONS:
    -i, --input <input>...    Glob matching one or more Rule files, to be used as the input files. Rules must be '.yml'
                              files
    -o, --output <output>     Path to write all matches, if path points to a directory then matches are written to files
                              named after the associated rules
    -r, --rules <rules>...    Glob matching one or more Rule files
```

### Input Glob example
```
$ tau-cli -i .test_data/*.json -r .test_data/*.yml
{"a":{"b":1}}
```

### Pipe example
```
$ cat .test_data/*.json | cargo run -- -r .test_data/*.yml
{"a":{"b":1}}
```

## Feature Plans
* Other input format options
    * XML
* Nicer output options
    * CSV of matched fields
    * Stats on matches / failures
