use marauder::unipen;
use std::env;
use std::fs;

// turn image drawing to hand sketch
///rainy stlye movie romantic
//https://convertimage.net/online-photo-effects/online-photo-drawing-sketch.asp

// ultra high contrast image to vector
//   https://vectormagic.com/pricing

// ascii art
//   https://www.text-image.com/convert/pic2ascii.cgi
///large large dezoomed

// xkcd drawing style transfer
//   https://jakevdp.github.io/blog/2012/10/07/xkcd-style-plots-in-matplotlib/
//https://matplotlib.org/xkcd/examples/showcase/xkcd.html
//

// xkcd font
//   https://stackoverflow.com/questions/39614381/how-to-get-xkcd-font-working-in-matplotlib
//https://news.ycombinator.com/item?id=9302740
//https://www.reddit.com/r/xkcd/comments/2i1i37/download_and_use_randalls_handwriting_as_a_font/
//https://xkcdsucks.blogspot.com/2009/03/xkcdsucks-is-proud-to-present-humor.html
///a tight columned GPT-3 monologue each finishing the other's sentence / writing the next paragraph, live
//https://github.com/ipython/xkcd-font

// turn image into xkcd drawing
//https://jakei.github.io/MyWebpage/sec_Projects/page_xkcd_style/index.html
///remarkable style GIFs 4realz
///future= animate the sketches in a theatral way/tight twitter loop
//https://davidwalsh.name/cmx-js

// render svg lines as a drawing
//   https://css-tricks.com/svg-line-animation-works/
//     animation-fill-mode  forwards, backwards, both, none

// rust svg file format as proto

// rust svg renderer
//   https://www.reddit.com/r/rust/comments/7knfnm/announcing_libresvg_an_svg_rendering_library/
//   https://github.com/RazrFalcon/resvg/search?q=antialiasing+is%3Aissue&type=Issues

// knowledge graph theory/UI
//https://zettelkasten.de/introduction/

// 2D pen/pencil/SVG printer software
//https://twitter.com/wblut/status/1265398379506475009?ref_src=twsrc%5Etfw%7Ctwcamp%5Etweetembed%7Ctwterm%5E1265398379506475009%7Ctwgr%5E%7Ctwcon%5Es1_&ref_url=https%3A%2F%2Fwblut.com%2Fsun-and-broken-crystal%2F
//https://axidraw.com/
//https://github.com/evil-mad/plotink
//https://github.com/evil-mad/axidraw/find/master <-- .svg
//https://axidraw.com/doc/cli_api/
// https://axidraw.com/doc/cli_api/#preview
//https://gitlab.com/oskay/hershey-text
// https://gitlab.com/oskay/svg-fonts
// https://gitlab.com/oskay/svg-fonts/-/blob/master/fonts/EMS/EMSCasualHand.svg

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: {:?} <ujipenchars.txt>", args[0]);
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
