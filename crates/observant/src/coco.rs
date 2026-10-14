//! The 80 COCO classes the YOLO models detect, by the class id they report; `classes` in the
//! settings names them.

/// Every class name, at its class id.
pub const NAMES: [&str; 80] = [
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "backpack",
    "umbrella",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "dining table",
    "toilet",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
];

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
        assert_eq!(id("toothbrush"), Some(79));
        assert_eq!(id("unicorn"), None);
        assert_eq!(name(15), Some("cat"));
        assert_eq!(name(80), None);
        assert_eq!(name(-1), None);
        for (i, n) in NAMES.iter().enumerate() {
            assert_eq!(id(n), Some(i as i64), "{n} appears once");
        }
    }
}
