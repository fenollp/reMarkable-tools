use std::str;

use itertools::Itertools;
use libremarkable::framebuffer::cgmath;
use nom::{
    bytes::complete::tag,
    character::complete::{
        char, digit1, line_ending, multispace0, multispace1, none_of, not_line_ending,
    },
    combinator::{map_res, opt, recognize},
    multi::{many0, many1, separated_list},
    sequence::{delimited, terminated, tuple},
    IResult,
};

#[derive(Debug, PartialEq)]
pub struct Word {
    pub glyph: String,
    pub id: String,
    pub strokes: Vec<Vec<cgmath::Point2<i16>>>,
}

fn comment(input: &str) -> IResult<&str, &str> {
    let (input, _) = multispace0(input)?;
    delimited(tag("//"), recognize(not_line_ending), line_ending)(input)
}

fn points_count(input: &str) -> IResult<&str, u16> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("POINTS")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, count) = map_res(recognize(digit1), str::parse)(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag("#")(input)?;
    Ok((input, count))
}

fn points(input: &str) -> IResult<&str, Vec<cgmath::Point2<i16>>> {
    let (input, _count) = points_count(input)?; // TODO: use count
    let (input, _) = multispace0(input)?;
    let (input, ints) = separated_list(
        multispace1,
        map_res(recognize(tuple((opt(char('-')), digit1))), str::parse),
    )(input)?;
    let points =
        ints.into_iter().tuple_windows().step_by(2).map(|(x, y)| cgmath::Point2 { x, y }).collect();
    Ok((input, points))
}

fn strokes_count(input: &str) -> IResult<&str, u8> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("NUMSTROKES")(input)?;
    let (input, _) = multispace1(input)?;
    map_res(digit1, str::parse)(input)
}

fn strokes(input: &str) -> IResult<&str, Vec<Vec<cgmath::Point2<i16>>>> {
    let (input, _count) = strokes_count(input)?; // TODO: use count
    let (input, _) = multispace0(input)?;
    separated_list(multispace1, points)(input)
}

fn word(input: &str) -> IResult<&str, Word> {
    let (input, _) = many0(comment)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("WORD")(input)?;
    let (input, glyph) =
        delimited(multispace1, recognize(many1(none_of(" "))), multispace1)(input)?;
    let (input, id) = terminated(recognize(not_line_ending), line_ending)(input)?;
    let (input, _) = multispace1(input)?;
    let (input, strokes) = strokes(input)?;
    let (input, _) = multispace0(input)?;
    Ok((input, Word { glyph: glyph.to_owned(), id: id.to_owned(), strokes }))
}

pub fn words(input: &str) -> IResult<&str, Vec<Word>> {
    many0(word)(input)
}

#[cfg(test)]
mod test {
    use libremarkable::cgmath::Point2;

    #[test]
    fn parse_comment() {
        assert_eq!(super::comment("// ASCII char: @\n"), Ok(("", " ASCII char: @")));
    }

    #[test]
    fn parse_points_count() {
        assert_eq!(super::points_count("  POINTS 114 #"), Ok(("", 114)));
    }

    #[test]
    fn parse_points() {
        assert_eq!(
            super::points("  POINTS 2 # 398 270 385 276"),
            Ok(("", vec![Point2 { x: 398, y: 270 }, Point2 { x: 385, y: 276 }]))
        );
    }

    #[test]
    fn parse_strokes_count() {
        assert_eq!(super::strokes_count("  NUMSTROKES 42"), Ok(("", 42)));
    }

    #[test]
    fn parse_strokes() {
        assert_eq!(
            super::strokes(
                "  NUMSTROKES 2
  POINTS 1 # 1 3
  POINTS 1 # 2 -4
"
            ),
            Ok(("\n", vec![vec![Point2 { x: 1, y: 3 }], vec![Point2 { x: 2, y: -4 }]]))
        );
    }

    #[test]
    fn parse_word() {
        let inp = "
// Non-ASCII char: euro
WORD € trn_UJI_W11-01
  NUMSTROKES 3
  POINTS 39 # 699 197 694 185 694 185 681 176 671 175 657 175 643 171 624 175 607 175 581 182 556 188 527 197 498 206 466 224 433 240 397 266 362 293 329 327 299 366 270 414 246 468 225 527 216 592 209 666 211 735 221 807 239 869 264 929 296 978 331 1020 370 1053 415 1068 458 1075 502 1071 535 1064 567 1043 593 1023 608 1010 617 989
  POINTS 25 # 156 489 145 493 145 493 141 504 141 504 154 507 165 513 184 507 206 507 233 498 264 492 299 479 335 473 372 461 410 456 445 449 474 442 500 438 520 436 531 440 531 440 536 458 520 478 505 497 481 522
  POINTS 17 # 62 773 48 787 48 787 53 800 63 809 85 811 106 817 136 814 171 814 214 811 254 802 295 799 339 786 379 777 411 774 442 775 464 777
";
        assert_eq!(
            super::word(inp),
            Ok((
                "",
                super::Word {
                    glyph: "€".to_owned(),
                    id: "trn_UJI_W11-01".to_owned(),
                    strokes: vec![
                        vec![
                            Point2 { x: 699, y: 197 },
                            Point2 { x: 694, y: 185 },
                            Point2 { x: 694, y: 185 },
                            Point2 { x: 681, y: 176 },
                            Point2 { x: 671, y: 175 },
                            Point2 { x: 657, y: 175 },
                            Point2 { x: 643, y: 171 },
                            Point2 { x: 624, y: 175 },
                            Point2 { x: 607, y: 175 },
                            Point2 { x: 581, y: 182 },
                            Point2 { x: 556, y: 188 },
                            Point2 { x: 527, y: 197 },
                            Point2 { x: 498, y: 206 },
                            Point2 { x: 466, y: 224 },
                            Point2 { x: 433, y: 240 },
                            Point2 { x: 397, y: 266 },
                            Point2 { x: 362, y: 293 },
                            Point2 { x: 329, y: 327 },
                            Point2 { x: 299, y: 366 },
                            Point2 { x: 270, y: 414 },
                            Point2 { x: 246, y: 468 },
                            Point2 { x: 225, y: 527 },
                            Point2 { x: 216, y: 592 },
                            Point2 { x: 209, y: 666 },
                            Point2 { x: 211, y: 735 },
                            Point2 { x: 221, y: 807 },
                            Point2 { x: 239, y: 869 },
                            Point2 { x: 264, y: 929 },
                            Point2 { x: 296, y: 978 },
                            Point2 { x: 331, y: 1020 },
                            Point2 { x: 370, y: 1053 },
                            Point2 { x: 415, y: 1068 },
                            Point2 { x: 458, y: 1075 },
                            Point2 { x: 502, y: 1071 },
                            Point2 { x: 535, y: 1064 },
                            Point2 { x: 567, y: 1043 },
                            Point2 { x: 593, y: 1023 },
                            Point2 { x: 608, y: 1010 },
                            Point2 { x: 617, y: 989 }
                        ],
                        vec![
                            Point2 { x: 156, y: 489 },
                            Point2 { x: 145, y: 493 },
                            Point2 { x: 145, y: 493 },
                            Point2 { x: 141, y: 504 },
                            Point2 { x: 141, y: 504 },
                            Point2 { x: 154, y: 507 },
                            Point2 { x: 165, y: 513 },
                            Point2 { x: 184, y: 507 },
                            Point2 { x: 206, y: 507 },
                            Point2 { x: 233, y: 498 },
                            Point2 { x: 264, y: 492 },
                            Point2 { x: 299, y: 479 },
                            Point2 { x: 335, y: 473 },
                            Point2 { x: 372, y: 461 },
                            Point2 { x: 410, y: 456 },
                            Point2 { x: 445, y: 449 },
                            Point2 { x: 474, y: 442 },
                            Point2 { x: 500, y: 438 },
                            Point2 { x: 520, y: 436 },
                            Point2 { x: 531, y: 440 },
                            Point2 { x: 531, y: 440 },
                            Point2 { x: 536, y: 458 },
                            Point2 { x: 520, y: 478 },
                            Point2 { x: 505, y: 497 },
                            Point2 { x: 481, y: 522 }
                        ],
                        vec![
                            Point2 { x: 62, y: 773 },
                            Point2 { x: 48, y: 787 },
                            Point2 { x: 48, y: 787 },
                            Point2 { x: 53, y: 800 },
                            Point2 { x: 63, y: 809 },
                            Point2 { x: 85, y: 811 },
                            Point2 { x: 106, y: 817 },
                            Point2 { x: 136, y: 814 },
                            Point2 { x: 171, y: 814 },
                            Point2 { x: 214, y: 811 },
                            Point2 { x: 254, y: 802 },
                            Point2 { x: 295, y: 799 },
                            Point2 { x: 339, y: 786 },
                            Point2 { x: 379, y: 777 },
                            Point2 { x: 411, y: 774 },
                            Point2 { x: 442, y: 775 },
                            Point2 { x: 464, y: 777 }
                        ],
                    ],
                }
            ))
        );
    }
}
