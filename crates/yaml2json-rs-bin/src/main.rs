use std::fs::File;
use std::io::{Read, Stderr, Stdout};
use std::path::Path;
use std::str::FromStr;
use std::{io, process};

use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};

use anyhow::bail;
use std::fmt::{Display, Formatter};
use yaml2json_rs::{Style, Yaml2Json};
use yaml_split::{DocumentIterator, YamlSplitError};

#[derive(Clone, clap::ValueEnum)]
enum ErrorStyle {
    Silent,
    Stderr,
    Json,
}

impl FromStr for ErrorStyle {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "silent" => Ok(ErrorStyle::Silent),
            "stderr" => Ok(ErrorStyle::Stderr),
            "json" => Ok(ErrorStyle::Json),
            _ => bail!("not a valid ErrorStyle"),
        }
    }
}

impl Display for ErrorStyle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ErrorStyle::Silent => "silent",
                ErrorStyle::Stderr => "stderr",
                ErrorStyle::Json => "json",
            }
        )
    }
}

/// `ErrorPrinter` allows you to configure how errors will be printed.
struct ErrorPrinter {
    pretty: bool,
    print_style: ErrorStyle,
    stdout: Stdout,
    stderr: Stderr,
}

impl ErrorPrinter {
    fn new(print_style: ErrorStyle, pretty: bool) -> Self {
        Self {
            pretty,
            print_style,
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }

    fn print(&mut self, d: impl Display) {
        match self.print_style {
            ErrorStyle::Silent => {}
            ErrorStyle::Stderr => write_or_exit(&mut self.stderr, &format!("{}\n", d)),
            ErrorStyle::Json => {
                let s = if self.pretty {
                    format!("{{\n  \"yaml-error\": \"{}\"\n}}\n", d)
                } else {
                    format!("{{\"yaml-error\":\"{}\"}}\n", d)
                };
                write_or_exit(&mut self.stdout, &s);
            }
        };
    }
}

// `write_or_exit` is used for writing to stdout / stderr
// as otherwise the program may panic.
// As this program's entire purpose is to write data to stdout / stderr
// any failure here means we should just exit cleanly with an error code.
fn write_or_exit(io: &mut dyn io::Write, s: &str) {
    let w = io.write(s.as_bytes());

    if w.is_err() {
        process::exit(1);
    }
}

fn write(yaml2json: &Yaml2Json, ep: &mut ErrorPrinter, read: impl Read) {
    let doc_iter = DocumentIterator::new(read);
    let mut printed_last = false;
    let mut stdout = io::stdout();

    for res in doc_iter {
        // print a newline between regular output lines
        if printed_last {
            write_or_exit(&mut stdout, "\n");
        }

        printed_last = false;

        match res {
            Ok(doc) => match yaml2json.document_to_writer(&doc, &mut stdout) {
                Ok(_) => printed_last = true,
                Err(e) => ep.print(e),
            },
            Err(e) => match e {
                // If there is an IOError, we should just exit.
                YamlSplitError::IOError(_) => process::exit(1),
            },
        }
    }

    if printed_last {
        // Add final newline
        write_or_exit(&mut stdout, "\n");
    }
}

fn main() {
    let usage = r#"./yaml2json file1.yaml file2.yaml

    cat file1.yaml | ./yaml2json

    ./yaml2json --error=json file1.yaml | jq"#;
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .usage(usage)
        .arg(
            Arg::with_name("pretty")
                .takes_value(false)
                .short('p')
                .long("pretty")
        )
        .arg(
            Arg::with_name("error")
                .takes_value(true)
                .short('e')
                .long("error")
                .default_value("json")
        )
        .arg(
            Arg::with_name("file")
                .help("Specify the path to files you want to convert. You can also pass files via stdin instead.")
                .multiple(true)
        )
        .get_matches();

    let fileopt = matches.values_of("file");
    let pretty = matches.is_present("pretty");
    let error: ErrorStyle = matches
        .value_of("error")
        .unwrap()
        .parse()
        .expect(r#"invalid error value, expected one of "silent", "stderr" or "json""#);

    let mut ep = ErrorPrinter::new(error, pretty);
    let yaml2json_style = if pretty {
        Style::PRETTY
    } else {
        Style::COMPACT
    };
    let yaml2json = Yaml2Json::new(yaml2json_style);

    // if: files are provided as arguments, read those instead of stdin
    if let Some(files) = fileopt {
        for f in files {
            let path = Path::new(f);

            if !path.exists() {
                ep.print(format!("file {} does not exist", path.display()));
            } else if path.is_dir() {
                ep.print(format!("{} is a directory", path.display()))
            } else {
                let file = File::open(f);

                match file {
                    Ok(f) => write(&yaml2json, &mut ep, f),
                    Err(e) => ep.print(e),
                }
            }
        }
    // else: No files provided as args, use stdin for input
    } else {
        let stdin = io::stdin();
        let stdin_lock = stdin.lock();

        write(&yaml2json, &mut ep, stdin_lock);
    }
}
