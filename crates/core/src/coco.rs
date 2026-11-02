//! The 80 COCO classes the detection model (DEIM-D-FINE-N) reports, by class id; `classes` in the
//! settings names them.

/// Every class name, at its class id: line N of the hub's shared COCO list is class N-1.
pub const NAMES: [&str; 80] = parse(include_str!("coco-classes.txt"));

/// Splits the list at compile time, so a short, long or CRLF copy fails the build instead of shifting ids.
const fn parse(list: &str) -> [&str; 80] {
    let mut names = [""; 80];
    let mut rest = list;
    let mut i = 0;
    while i < names.len() {
        let bytes = rest.as_bytes();
        let mut end = 0;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        assert!(
            end > 0 && end < bytes.len(),
            "coco-classes.txt needs 80 non-empty LF lines"
        );
        assert!(
            bytes[end - 1] != b'\r',
            "coco-classes.txt must keep LF line ends"
        );
        let (line, tail) = rest.split_at(end);
        names[i] = line;
        rest = tail.split_at(1).1;
        i += 1;
    }
    assert!(rest.is_empty(), "coco-classes.txt has more than 80 lines");
    names
}

/// The class id of `name`, or `None` for one the models do not know.
pub fn id(name: &str) -> Option<i64> {
    NAMES.iter().position(|n| *n == name).map(|i| i as i64)
}

/// The name of class `id`.
pub fn name(id: i64) -> Option<&'static str> {
    usize::try_from(id).ok().and_then(|i| NAMES.get(i)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_map_both_ways_and_cat_is_fifteen() {
        assert_eq!(id("cat"), Some(15));
        assert_eq!(id("person"), Some(0));
        assert_eq!(id("hair drier"), Some(78));
        assert_eq!(name(79).and_then(id), Some(79));
        assert_eq!(id("unicorn"), None);
        assert_eq!(name(15), Some("cat"));
        assert_eq!(name(80), None);
        assert_eq!(name(-1), None);
        for (i, n) in NAMES.iter().enumerate() {
            assert_eq!(id(n), Some(i as i64), "{n} appears once");
        }
    }

    fn lines(count: usize, end: &str) -> String {
        (0..count).map(|i| format!("class{i}{end}")).collect()
    }

    #[test]
    fn eighty_lf_lines_parse_in_order() {
        assert_eq!(parse(&lines(80, "\n"))[79], "class79");
    }

    #[test]
    #[should_panic(expected = "80 non-empty LF lines")]
    fn a_short_list_is_refused() {
        parse(&lines(79, "\n"));
    }

    #[test]
    #[should_panic(expected = "more than 80 lines")]
    fn a_long_list_is_refused() {
        parse(&lines(81, "\n"));
    }

    #[test]
    #[should_panic(expected = "keep LF line ends")]
    fn a_crlf_list_is_refused() {
        parse(&lines(80, "\r\n"));
    }
}
