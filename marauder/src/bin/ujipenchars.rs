use std::{env, fs};

use marauder::unipen;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: {:?} <ujipenchars.txt>", args[0]);
        println!("The dataset can be downloaded from https://archive.ics.uci.edu/ml/machine-learning-databases/uji-penchars/version2/ujipenchars2.txt");
        std::process::exit(1);
    }

    let contents = fs::read_to_string(&args[1]).expect("Something went wrong reading the file");
    println!("Read {} bytes", contents.len());

    match unipen::words(&contents) {
        Ok((rest, words)) => {
            if !rest.is_empty() {
                println!("rest: .{:?}.", &rest[0..10]);
            }
            println!("parsed {:?} UNIPEN words", words.len());
            std::process::exit(0)
        }
        err => {
            println!("Parse error: {:?}", err);
            std::process::exit(1)
        }
    }
}
