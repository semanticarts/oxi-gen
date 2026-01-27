use oxi_tarql::OxiTarql;
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, command, value_parser};
use std::time::Instant;

fn parse_args() -> ArgMatches {
    command!()
        .about("Convert CSV file to RDF using SPARQL")
        .arg(
            Arg::new("delimiter")
                .short('d')
                .long("delimiter")
                .default_value(",")
                .conflicts_with("tab")
                .help("Delimiting character of the input file"),
        )
        .arg(
            Arg::new("tab")
                .short('t')
                .long("tab")
                .action(ArgAction::SetTrue)
                .conflicts_with("delimiter")
                .help("Is the Input tab-separated (TSV)?"),
        )
        .arg(
            Arg::new("escape_char")
                .short('p')
                .long("escape_char")
                .default_value("\\")
                .help("Escape character used in the input file"),
        )
        .arg(
            Arg::new("quote_char")
                .long("quote_char")
                .default_value("\"")
                .help("Quote character used in the input file"),
        )
        .arg(
            Arg::new("normalize")
                .short('n')
                .long("normalize")
                .action(ArgAction::SetTrue)
                .help(
                    "Normalize column names - convert all to UPPERCASE [default: no normalization]",
                ),
        )
        .arg(
            Arg::new("headers")
                .short('H')
                .long("no-header-row")
                .action(ArgAction::SetFalse)
                .help("File has headers in the first row [default: True]"),
        )
        .arg(
            Arg::new("gzip")
                .short('g')
                .long("gzip")
                .action(ArgAction::SetTrue)
                .requires("output")
                .help("gzip file output. Output file name must be provided"),
        )
        .arg(
            Arg::new("ntriples")
                .long("ntriples")
                .action(ArgAction::SetTrue)
                .help("Emit N-Triples [default: turtle]"),
        )
        .arg(
            Arg::new("quads")
                .long("quads")
                .requires("name")
                .action(ArgAction::SetTrue)
                .help("Output quads (trig). Use --name for graph URI"),
        )
        .group(ArgGroup::new("types").args(["ntriples", "quads"]))
        .arg(
            Arg::new("name")
                .long("name")
                .action(ArgAction::Set)
                .default_value("")
                .help("Named graph URI "),
        )
        .arg(
            Arg::new("test")
                .long("test")
                .value_parser(value_parser!(u32).range(1..50))
                .action(ArgAction::Set)
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("5")
                .help("Show output for first TEST records (default=5)"),
        )
        .arg(
            Arg::new("split")
                .long("split")
                .action(ArgAction::Append)
                .num_args(3)
                .value_names(["ORIGINAL", "SPLIT", "DELIMITER"])
                .help("Split column ORIGINAL into multiple values in SPLIT on DELIMITER")
        )
        .arg(
            Arg::new("dedup")
                .long("dedup")
                .value_parser(value_parser!(u32).range(1000..=5000000))
                .default_missing_value("1000")
                .num_args(0..=1)
                .require_equals(true)
                .action(ArgAction::Set)
                .help("Window size in which to remove duplicate triples (default=1000)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .action(ArgAction::Set)
                .default_value("STDOUT")
                .help("File to write to, omit to use STDOUT"),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .action(ArgAction::Set)
                .default_value("STDIN")
                .help("CSV to be processed, omit to use STDIN"),
        )
        .arg(
            Arg::new("query")
                .short('q')
                .long("query")
                .action(ArgAction::Set)
                .required(true)
                .help("File containing a SPARQL query to be applied to an input file (required)"),
        )
        .get_matches()
}

fn main() {
    // parse the supplied arguments
    let matches = parse_args();

    let split_def = match matches.get_many::<String>("split") {
        None => vec![],
        Some(splits) => {
            let mut sval_it = splits.map(|sd| sd.clone());
            let mut split_defs = Vec::<(String, String, String)>::new();
            while let Some(orig) = sval_it.next() {
                split_defs.push((orig, sval_it.next().unwrap(), sval_it.next().unwrap()));
            }
            split_defs
        }
    };

    let mut tarql = OxiTarql {
        delimiter: matches.get_one::<String>("delimiter").unwrap().to_string(),
        tab: matches.get_flag("tab"),
        test: match matches.get_one::<u32>("test") {
            None => 0,
            Some(t) => *t
        },
        headers: matches.get_flag("headers"),
        escape_char: matches
            .get_one::<String>("escape_char")
            .unwrap()
            .to_string(),
        quote_char: matches.get_one::<String>("quote_char").unwrap().to_string(),
        normalize: matches.get_flag("normalize"),
        gzip: matches.get_flag("gzip"),
        quads: matches.get_flag("quads"),
        ntriples: matches.get_flag("ntriples"),
        dedup: match matches.get_one::<u32>("dedup") {
            None => 0,
            Some(t) => *t
        },
        named_graph: matches.get_one::<String>("name").unwrap().to_string(),
        input: matches.get_one::<String>("input").unwrap().to_string(),
        output: matches.get_one::<String>("output").unwrap().to_string(),
        query: matches.get_one::<String>("query").unwrap().to_string(),
        split: split_def
    };

    let start = Instant::now();

    tarql.transform().expect("Oops something went wrong");

    let duration = start.duration_since(start);
    eprintln!("Processing complete in {} seconds", duration.as_secs_f32());
}
