use std::{error::Error, fs, process, env};

use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use spargebra::SparqlParser;
use spareval::QueryEvaluator;
// use oxigraph::spareval::SparqlEvaluator;
// use spargebra::SparqlParser;

fn transform(query_path: &String, input: &String) -> Result<(), Box<dyn Error>> {
    // Build the CSV reader and iterate over each record.
    // println!("Query {:?}", query_path);
    // println!("Input {:?}", input);
    let mut rdr = csv::Reader::from_path(input).unwrap();
    let mut _row = 0;

    let store = Dataset::new();
    let evaluator = QueryEvaluator::new();
    let query_str = fs::read_to_string(&query_path).unwrap();
    let query = SparqlParser::new().parse_query(&query_str).unwrap();

    // let headers = Vec::from_iter(rdr.headers());
    let headers: Vec<String> = rdr.headers()?.into_iter().map(String::from)
        //.flat_map(Variable::new)
        .collect();
    for result in rdr.records() {
        // The iterator yields Result<StringRecord, Error>, so we check the
        // error here.
        let record = result?;
        // println!("{:?}", headers);
        // println!("{:?}", record);
        let mut prepared = evaluator.prepare(&query);
        for (varname, value) in headers.iter().zip(record.iter()) {
            prepared = prepared.substitute_variable(Variable::new(varname)?, Literal::from(value));
        }

        let results = prepared.execute(&store);
        if let QueryResults::Graph(triples) = results.unwrap() {
            for triple in triples {
                println!("{}", triple?);
            }
        }

        _row += 1;
        // if row > 10 {
        //     break;
        // }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(err) = transform(&args[1], &args[2]) {
        println!("error running example: {}", err);
        process::exit(1);
    }
}