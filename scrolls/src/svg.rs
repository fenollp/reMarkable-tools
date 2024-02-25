use std::io::BufRead;

use anyhow::Result;
use libremarkable::appctx::ApplicationContext;
use log::info;
use pb::proto::hypercards::{drawing, Drawing};
use quick_xml::events::Event;
use svg_path_parser::parse_with_resolution;
use tokio::time::sleep;

use crate::paint::{paint_nopause, DRAWING_PACE, INTER_DRAWING_PACE};

// pub fn parse(svg: &str, tol: f64, preprocess: bool) -> Result<Vec<Polyline>, Error> {
//     // Preprocess and simplify the SVG using the usvg library
//     let svg = if preprocess {
//         let usvg_input_options = usvg::Options::default();
//         let usvg_tree = usvg::Tree::from_str(svg, &usvg_input_options.to_ref())?;
//         let usvg_xml_options = usvg::XmlOptions::default();
//         usvg_tree.to_string(&usvg_xml_options)
//     } else {
//         svg.to_string()
//     };

pub(crate) async fn read_and_paint(app: &mut ApplicationContext<'_>, fpath: String) -> Result<()> {
    let mut reader = quick_xml::Reader::from_file(fpath)?;
    reader.trim_text(true);

    for (path, _transform) in browse(reader)? {
        // A closed line has the ending point connect to the starting point.
        for (_closed, line) in parse_with_resolution(path.as_str(), 64) {
            info!(target:env!("CARGO_PKG_NAME"), "drawing XxY: {n}x{n}", n = line.len());

            let d = into_drawing(line);

            paint_nopause(app, &d);

            sleep(if false { DRAWING_PACE } else { INTER_DRAWING_PACE }).await;
        }
        sleep(if true { DRAWING_PACE } else { INTER_DRAWING_PACE }).await;
    }
    Ok(())
}

//parse axidraw's and the font SVGs

fn browse<T: BufRead>(mut reader: quick_xml::Reader<T>) -> Result<Vec<(String, Option<String>)>> {
    let mut paths = vec![];

    let mut buf = vec![];
    loop {
        match reader.read_event(&mut buf)? {
            Event::Eof => break,
            // TODO: to support `transform=""`, one should read the parent groups `<g transform="translate(0,5.9000033)" ..>..</g>`
            Event::Start(ref e) | Event::Empty(ref e) if matches!(e.name(), b"glyph" | b"path") => {
                let mut path_expr: Option<String> = None;
                let mut transform_expr: Option<String> = None;
                for attr in e.attributes().filter_map(Result::ok) {
                    let extract = || {
                        attr.unescaped_value()
                            .ok()
                            .and_then(|v| std::str::from_utf8(&v).map(str::to_string).ok())
                    };
                    match attr.key {
                        b"d" => path_expr = extract(),
                        b"transform" => transform_expr = extract(),
                        _ => {}
                    }
                }
                if let Some(path) = path_expr {
                    // https://github.com/dbrgn/svg2polylines/blob/02eae484f39409e21cb1bcdba0f2dd065633c4a8/src/lib.rs#L773
                    paths.push((path, transform_expr));
                }
            }
            _ => {}
        }

        // If we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
        buf.clear();
    }
    Ok(paths)
}

fn into_drawing(line: Vec<(f64, f64)>) -> Drawing {
    const PRESSURE: i32 = 2000;
    const WIDTH: u32 = 2;
    const OFF_X: f64 = 0.;
    const MUL_X: f64 = 1.;
    const OFF_Y: f64 = 0.;
    const MUL_Y: f64 = MUL_X;

    Drawing {
        xs: line.iter().map(|(x, _)| (MUL_X * (x + OFF_X)) as f32).collect(),
        ys: line.iter().map(|(_, y)| (MUL_Y * (y + OFF_Y)) as f32).collect(),
        pressures: line.iter().map(|_| PRESSURE).collect(),
        widths: line.iter().map(|_| WIDTH).collect(),
        color: drawing::Color::Black as i32,
    }
}

#[test]
fn reads_a_drawing_from_svg_paths() {
    let svg = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!-- Created with Inkscape (http://www.inkscape.org/) -->

<svg
   xmlns:dc="http://purl.org/dc/elements/1.1/"
   xmlns:cc="http://creativecommons.org/ns#"
   xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
   xmlns:svg="http://www.w3.org/2000/svg"
   xmlns="http://www.w3.org/2000/svg"
   xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
   xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
   width="11in"
   height="8.5in"
   viewBox="0 0 279.40001 215.9"
   id="svg4258"
   version="1.1"
   inkscape:version="0.91 r13725"
   sodipodi:docname="wiresphere.svg">
  <sodipodi:namedview
     inkscape:document-units="in"
     pagecolor="\#ffffff"
     bordercolor="\#666666"
     borderopacity="1.0"
     inkscape:pageopacity="0.0"
     inkscape:pageshadow="2"
     inkscape:zoom="0.85359477"
     inkscape:cx="493.82848"
     inkscape:cy="382.5"
     inkscape:current-layer="layer1"
     id="namedview4262"
     showgrid="false"
     inkscape:window-width="1436"
     inkscape:window-height="855"
     inkscape:window-x="0"
     inkscape:window-y="1"
     inkscape:window-maximized="1"
     units="in" />
  <defs
     id="defs4260" />
  <metadata
     id="metadata4264">
    <rdf:RDF>
      <cc:Work
         rdf:about="">
        <dc:format>image/svg+xml</dc:format>
        <dc:type
           rdf:resource="http://purl.org/dc/dcmitype/StillImage" />
        <dc:title></dc:title>
      </cc:Work>
    </rdf:RDF>
  </metadata>
  <g
     inkscape:label="Layer 1"
     inkscape:groupmode="layer"
     id="layer1"
     transform="translate(0,5.9000033)">
    <g
       transform="matrix(0.80669983,0,0,0.80669983,139.70001,102.05)"
       inkscape:label="WireframeSphere"
       id="g11622">
      <g
         inkscape:label="Lines of Longitude"
         id="g11624">
        <path
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           transform="matrix(0.99676669,0.08035027,-0.08035027,0.99676669,0,0)"
           sodipodi:cx="0"
           sodipodi:cy="0"
           sodipodi:end="4.712389"
           sodipodi:open="true"
           sodipodi:rx="12.870777"
           sodipodi:ry="112.88889"
           sodipodi:start="1.5707963"
           sodipodi:type="arc"
           id="path11626"
           d="M 3.4487114e-7,112.88889 A 12.870777,112.88889 0 0 1 -11.14642,56.444448 a 12.870777,112.88889 0 0 1 0,-112.888895 A 12.870777,112.88889 0 0 1 2.5246428e-7,-112.88889" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11764"
           cx="0"
           cy="-78.026596"
           rx="60.587036"
           ry="34.752296" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11766"
           cx="0"
           cy="-80.478027"
           rx="55.603203"
           ry="31.893688" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11768"
           cx="0"
           cy="-82.716049"
           rx="50.47192"
           ry="28.950504" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11770"
           cx="0"
           cy="-84.734726"
           rx="45.206795"
           ry="25.930553" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11772"
           cx="0"
           cy="-86.528694"
           rx="39.821793"
           ry="22.841841" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11774"
           cx="0"
           cy="-88.093208"
           rx="34.331184"
           ry="19.692558" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11776"
           cx="0"
           cy="-89.42411"
           rx="28.74954"
           ry="16.491058" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11778"
           cx="0"
           cy="-90.517883"
           rx="23.091656"
           ry="13.24583" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11780"
           cx="0"
           cy="-91.371613"
           rx="17.372536"
           ry="9.965477" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11782"
           cx="0"
           cy="-91.983047"
           rx="11.607348"
           ry="6.6587014" />
        <ellipse
           style="fill:none;stroke:#000000;stroke-width:0.34984785"
           id="path11784"
           cx="0"
           cy="-92.350555"
           rx="5.8113794"
           ry="3.3342702" />
      </g>
      <circle
         style="fill:none;stroke:#000000;stroke-width:0.34984785"
         id="path11786"
         cx="0"
         cy="0"
         r="112.88889" />
    </g>
  </g>
</svg>
"#;
    let expected_path = "M 3.4487114e-7,112.88889 A 12.870777,112.88889 0 0 1 -11.14642,56.444448 a 12.870777,112.88889 0 0 1 0,-112.888895 A 12.870777,112.88889 0 0 1 2.5246428e-7,-112.88889";

    let mut reader = quick_xml::Reader::from_str(svg);
    reader.trim_text(true);

    let paths = browse(reader).unwrap().into_iter().map(|(p, _)| p).collect::<Vec<_>>();
    assert_eq!(paths, vec![expected_path]);

    let lines = parse_with_resolution(expected_path, 64).collect::<Vec<_>>();
    assert_eq!(lines.len(), 0); // What??

    assert!(svg2polylines::parse(expected_path, 0.15, true).is_err());
}

#[test]
fn reads_a_drawing_from_svg_glyphs() {
    let svg = r#"<?xml version="1.0" encoding="UTF-8" ?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" >

<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1">

<metadata>
Font name:               EMS Delight Swash Caps
License:                 SIL Open Font License http://scripts.sil.org/OFL
Created by:              Sheldon B. Michaels
SVG font conversion by:  Windell H. Oskay
A derivative of:         Delius Swash Caps
Designer:                Natalia Raices
Google font page:        https://fonts.google.com/specimen/Delius+Swash+Caps
</metadata>
<defs>
<font id="EMSDelightSwashCaps" horiz-adv-x="378" >
<font-face
font-family="EMS Delight Swash Caps"
units-per-em="1000"
ascent="800"
descent="-200"
cap-height="500"
x-height="300"
/>
<missing-glyph horiz-adv-x="378" />
<glyph unicode=" " glyph-name="space" horiz-adv-x="378" />
<glyph unicode="!" glyph-name="exclam" horiz-adv-x="198" d="M 159 680 L 159 195 M 144 47.2 L 140 3.15 L 172 6.3 L 172 47.2 L 144 47.2" />
<glyph unicode="+" glyph-name="plus" horiz-adv-x="457" d="M 81.9 230 L 432 230 M 255 406 L 255 47.2" />
<glyph unicode="-" glyph-name="hyphen" horiz-adv-x="369" d="M 91.4 230 L 321 230" />
<glyph unicode="." glyph-name="period" horiz-adv-x="167" d="M 91.4 50.4 L 88.2 18.9 L 113 15.8 L 117 47.2 L 91.4 50.4" />
<glyph unicode=":" glyph-name="colon" horiz-adv-x="186" d="M 107 419 L 113 378 L 129 381 L 129 416 L 107 419 M 101 50.4 L 97.6 9.45 L 126 9.45 L 126 53.6 L 101 50.4" />
<glyph unicode="=" glyph-name="equal" horiz-adv-x="523" d="M 104 296 L 457 296 M 97.6 164 L 460 164" />
<glyph unicode="L" glyph-name="L" horiz-adv-x="548" d="M 158 662 L 176 428 L 176 287 L 173 142 L 164 94.5 L 151 50.4 L 132 18.9 L 104 15.8 L 85 25.2 L 85 40.9 L 94.5 53.6 L 123 47.2 L 158 44.1 L 274 12.6 L 397 18.9 L 476 53.6 L 517 107 L 523 151 L 498 198 L 457 217 L 406 217 L 378 198 L 372 161" />
<glyph unicode="|" glyph-name="bar" horiz-adv-x="176" d="M 107 706 L 107 -236" />
<glyph unicode="&#xE6;" glyph-name="ae" horiz-adv-x="790" d="M90.934 380.72C249.656 495.275 393.374 416.706 414.309 267.72C435.244 118.734 352.343 17.4862 248.934 8.97017C145.525 0.454184 86.6368 71.3227 99.0603 147.408C110.863 219.69 200.384 251.749 417.967 221.86C635.55 191.971 725.071 224.03 736.874 296.312C749.297 372.397 690.409 443.265 587 434.749C483.591 426.233 400.69 324.986 421.625 176C442.56 27.0141 586.278 -51.5557 745 63" />
</font>
</defs>
</svg>
"#;

    let mut reader = quick_xml::Reader::from_str(svg);
    reader.trim_text(true);

    let path_pipe = "M 107 706 L 107 -236";
    let path_z = "M 78.8 422 L 378 422 L 72.5 18.9 L 400 18.9";

    let paths = browse(reader).unwrap().into_iter().map(|(p, _)| p).collect::<Vec<_>>();
    assert_eq!(paths, vec![
        "M 159 680 L 159 195 M 144 47.2 L 140 3.15 L 172 6.3 L 172 47.2 L 144 47.2",
        "M 81.9 230 L 432 230 M 255 406 L 255 47.2",
        "M 91.4 230 L 321 230",
        "M 91.4 50.4 L 88.2 18.9 L 113 15.8 L 117 47.2 L 91.4 50.4",
        "M 107 419 L 113 378 L 129 381 L 129 416 L 107 419 M 101 50.4 L 97.6 9.45 L 126 9.45 L 126 53.6 L 101 50.4",
        "M 104 296 L 457 296 M 97.6 164 L 460 164",
        "M 158 662 L 176 428 L 176 287 L 173 142 L 164 94.5 L 151 50.4 L 132 18.9 L 104 15.8 L 85 25.2 L 85 40.9 L 94.5 53.6 L 123 47.2 L 158 44.1 L 274 12.6 L 397 18.9 L 476 53.6 L 517 107 L 523 151 L 498 198 L 457 217 L 406 217 L 378 198 L 372 161",
        path_pipe,
        "M90.934 380.72C249.656 495.275 393.374 416.706 414.309 267.72C435.244 118.734 352.343 17.4862 248.934 8.97017C145.525 0.454184 86.6368 71.3227 99.0603 147.408C110.863 219.69 200.384 251.749 417.967 221.86C635.55 191.971 725.071 224.03 736.874 296.312C749.297 372.397 690.409 443.265 587 434.749C483.591 426.233 400.69 324.986 421.625 176C442.56 27.0141 586.278 -51.5557 745 63",
        ]);

    // assert!(svg2polylines::parse(path_pipe, 0.15, true).is_err());
    assert_eq!(svg2polylines::parse(svg, 0.15, true).unwrap(), vec![]);

    let lines_pipe = parse_with_resolution(path_pipe, 64).collect::<Vec<_>>();
    assert_eq!(lines_pipe.len(), 1);
    assert!(!lines_pipe[0].0);
    assert_eq!(
        into_drawing(lines_pipe[0].clone().1),
        Drawing {
            xs: [107.0, 107.0].into(), //B-b-b-but that's just noop
            ys: [706.0, -236.0].into(),
            pressures: [2000, 2000].into(),
            widths: [2, 2].into(),
            color: drawing::Color::Black.into(),
        }
    );

    use vsvg::{IntoBezPath, Point, Polyline};
    // let poly = Polyline::new(vec![Point::new(107.0, 706.0), Point::new(107.0, -236.0)]);
    // dbg!(poly.clone().into_bezpath());
    // // dbg!(poly.clone().into_bezpath_with_tolerance(10.0 * DEFAULT_TOLERANCE));
    // // dbg!(poly.clone().into_bezpath_with_tolerance(-10.0 * DEFAULT_TOLERANCE));
    // // dbg!(poly.clone().into_bezpath_with_tolerance(DEFAULT_TOLERANCE / 10.0));
    // assert_eq!(poly.into_points(), vec![]);
    // [scrolls/src/svg.rs:360] poly.clone().into_bezpath() = BezPath(
    //     [
    //         MoveTo(
    //             (107.0, 706.0),
    //         ),
    //         LineTo(
    //             (107.0, -236.0),
    //         ),
    //     ],
    // )
    // thread 'svg::reads_a_drawing_from_svg_glyphs' panicked at scrolls/src/svg.rs:364:5:
    // assertion `left == right` failed
    //   left: [Point { data: [107.0, 706.0] }, Point { data: [107.0, -236.0] }]
    //  right: []

    // https://docs.rs/vsvg/latest/vsvg/struct.Polyline.html

    let lines_z = parse_with_resolution(path_z, 64).collect::<Vec<_>>();
    // let poly = Polyline::new(vec![Point::new(107.0, 706.0), Point::new(107.0, -236.0)]);
    let poly = Polyline::new(lines_z[0].1.iter().map(|(x, y)| Point::new(*x, *y)).collect());
    assert_eq!(
        poly.clone().into_points(),
        vec![
            Point::new(78.8, 422.0),
            Point::new(378.0, 422.0),
            Point::new(72.5, 18.9),
            Point::new(400.0, 18.9)
        ]
    );
    assert_eq!(poly.into_bezpath(), {
        use vsvg::exports::kurbo::BezPath;

        let mut b = BezPath::new();
        b.move_to((78.8, 422.0));
        b.line_to((378.0, 422.0));
        b.line_to((72.5, 18.9));
        b.line_to((400.0, 18.9));
        b
    });

    assert_eq!(lines_z.len(), 1);
    assert!(!lines_z[0].0);
    assert_eq!(
        lines_z.into_iter().map(|(c, v)| { (c, into_drawing(v)) }).collect::<Vec<_>>(),
        vec![(
            false,
            Drawing {
                xs: [78.8, 378.0, 72.5, 400.0].into(),
                ys: [422.0, 422.0, 18.9, 18.9].into(),
                pressures: [2000, 2000, 2000, 2000,].into(),
                widths: [2, 2, 2, 2,].into(),
                color: drawing::Color::Black.into(),
            }
        )]
    );
}

// https://github.com/dbrgn/svg2polylines/blob/main/src/lib.rs

// https://docs.rs/vsvg/latest/vsvg/struct.Polyline.html

// https://docs.rs/vcr-cassette/latest/vcr_cassette/
