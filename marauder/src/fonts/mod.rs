use nom::bytes::complete::tag;
use nom::multi::separated_list;
use nom::multi::separated_nonempty_list;
use nom::multi::{many0, many1};
use nom::number::complete::float;
use nom::sequence::tuple;
use nom::IResult;
use quick_xml::de::{from_str, DeError};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, PartialEq)]
struct Svg {
    defs: Defs,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Defs {
    font: AFont,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AFont {
    #[serde(rename = "glyph", default)]
    glyphs: Vec<Glyph>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Glyph {
    unicode: String,
    d: Option<String>,
}

fn ws(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag(" ")(input)?;
    Ok((input, ()))
}

fn point(input: &str) -> IResult<&str, (f32, f32)> {
    let (input, (_, x, _, y)) = tuple((many0(ws), float, many1(ws), float))(input)?;
    Ok((input, (x, y)))
}

fn points(input: &str) -> IResult<&str, Vec<(f32, f32)>> {
    let el = tuple((many0(ws), tag("L"), many0(ws)));
    separated_list(el, point)(input)
}

fn vec_of_points(input: &str) -> IResult<&str, Vec<Vec<(f32, f32)>>> {
    let em = tuple((many0(ws), tag("M")));
    let (input, _) = em(input)?;
    separated_nonempty_list(em, points)(input)
}

pub type Font = HashMap<String, Vec<Vec<(f32, f32)>>>;

pub fn emsdelight_swash_caps() -> Result<Font, DeError> {
    let svg: Svg = from_str(include_str!("./EMSDelightSwashCaps.svg"))?;
    let mut glyphs = HashMap::with_capacity(svg.defs.font.glyphs.len());
    for glyph in &svg.defs.font.glyphs {
        if let Some(d) = &glyph.d {
            // We're doing a terrible job at parsing SVG paths and painting lines
            // when we should be painting Bezier curves from SVG paths...
            // Anyway let's skip the less basic path instructions (other than M L)
            // c.f. https://developer.mozilla.org/en-US/docs/Web/SVG/Tutorial/Paths
            if d.contains('C') {
                continue;
            }
            let (rest, dd) = vec_of_points(d).unwrap();
            assert_eq!(rest, "");
            glyphs.insert(glyph.unicode.to_owned(), dd);
        }
    }
    Ok(glyphs)
}

#[cfg(test)]
mod test {

    #[test]
    fn emsdelight_swash_caps_result() {
        let glyphs = super::emsdelight_swash_caps().unwrap();
        let i = vec![
            vec![(117., 438.), (117., 9.45)],
            vec![
                (94.5, 630.),
                (94.5, 589.),
                (126., 586.),
                (123., 630.),
                (94.5, 630.),
            ],
        ];
        assert_eq!(glyphs.get("i"), Some(&i));
        assert_eq!(glyphs.len(), 206 - 2 - 21);
    }

    #[test]
    fn parse_point() {
        assert_eq!(super::point(" 159 680"), Ok(("", (159., 680.))));
    }

    #[test]
    fn parse_points() {
        assert_eq!(
            super::points(" 159 680 L 159 195"),
            Ok(("", vec![(159., 680.), (159., 195.)]))
        );
    }

    #[test]
    fn parse_stroke() {
        assert_eq!(
            super::vec_of_points("M 159 680 L 159 195"),
            Ok(("", vec![vec![(159., 680.), (159., 195.)],]))
        );
    }

    #[test]
    fn parse_d() {
        assert_eq!(
            super::vec_of_points(
                "M 159 680 L 159 195 M 144 47.2 L 140 3.15 L 172 6.3 L 172 47.2 L 144 47.2"
            ),
            Ok((
                "",
                vec![
                    vec![(159., 680.), (159., 195.)],
                    vec![
                        (144., 47.2),
                        (140., 3.15),
                        (172., 6.3),
                        (172., 47.2),
                        (144., 47.2)
                    ],
                ]
            ))
        );
    }
}
