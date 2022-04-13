// rust one-dimensional binary cellular automaton
// ideally a la wolfram (top down)

////////////////////////////////////////////////////////////////////////////////////////////////////////////////

extern crate rand;

use rand::distributions::Uniform;
use rand::{thread_rng, Rng};

fn main() {
    println!("{}", "Sierpiński triangle");
    {
        let mut grid = vec![0, 0, 0, 0, 1, 0, 0, 0, 0]; //Sierpiński triangle
        let generations = 10;
        println!["{:?}", grid];

        for _ in 0..generations {
            grid = next_gen(grid);
            println!["{:?}", grid];
        }
    }

    println!("{}", "random grid");
    {
        let mut grid = random_grid(9);
        let generations = 10;
        println!("{:?}", grid);
        for _ in 0..generations {
            grid = next_gen(grid);
            println!["{:?}", grid];
        }
    }
}

//currently one-dimensional
//consider using ndarray
fn random_grid(len: usize) -> Vec<u8> {
    let roll_range = Uniform::new_inclusive(0, 1);
    thread_rng().sample_iter(&roll_range).take(len).collect()
}

fn next_gen(old: Vec<u8>) -> Vec<u8> {
    let mut new = old.to_vec();

    //border operations
    new[0] = procreate(0, old[0], old[1]).translate();
    new[old.len() - 1] = procreate(old[old.len() - 2], old[old.len() - 1], 0).translate();

    //regular operations
    for cell_ind in 1..(old.len() - 1) {
        let genome = procreate(old[cell_ind - 1], old[cell_ind], old[cell_ind + 1]);
        // println!["{}'s genome is {}", cell_ind, genome.code]; //just for debugging
        new[cell_ind] = genome.translate();
    }
    new
}

fn procreate(a: u8, b: u8, c: u8) -> Genome {
    Genome {
        code: 1 * a + 2 * b + 4 * c,
    }
}

struct Genome {
    code: u8,
}

impl Genome {
    fn translate(&self) -> u8 {
        match self.code {
            0 => 0,
            1 => 1,
            2 => 0,
            3 => 1,
            4 => 1,
            5 => 0,
            6 => 1,
            7 => 0,
            _ => {
                panic!["you messed up"];
            }
        }
    }
}


////////////////////////////////////////////////////////////////////////////////////////////////////////////////

fn get_new_state(windowed: &[bool]) -> bool {
    match windowed {
        [false, true, true] | [true, true, false] => true,
        _ => false
    }
}
 
fn next_gen(cell: &mut [bool]) {
    let mut v = Vec::with_capacity(cell.len());
    v.push(cell[0]);
    for i in cell.windows(3) {
        v.push(get_new_state(i));
    }
    v.push(cell[cell.len() - 1]);
    cell.copy_from_slice(&v);
}
 
fn print_cell(cell: &[bool]) {
    for v in cell {
        print!("{} ", if *v {'#'} else {' '});
    }
    println!();
}
 
fn main() {
 
    const MAX_GENERATION: usize = 10;
    const CELLS_LENGTH: usize = 30;
 
    let mut cell: [bool; CELLS_LENGTH] = rand::random();
 
    for i in 1..=MAX_GENERATION {
        print!("Gen {:2}: ", i);
        print_cell(&cell);
        next_gen(&mut cell);
    }
}
 
