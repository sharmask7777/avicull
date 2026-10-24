use avicull::detector::*;
use image::{DynamicImage, Rgb, RgbImage};

#[test]
fn test_detector_zero_detections() {
    let img = DynamicImage::ImageRgb8(RgbImage::new(100, 100));
    let detections = vec![];
    let crop = select_and_pad_bird_crop(&img, &detections, 0.25, (224, 224));
    assert!(crop.is_none(), "Zero detections must return None");
}

#[test]
fn test_detector_filter_below_min_detect() {
    let img = DynamicImage::ImageRgb8(RgbImage::new(100, 100));
    let detections = vec![
        Detection {
            class_id: 14,     // bird
            confidence: 0.20, // < 0.25
            bbox_xyxy: [10.0, 10.0, 50.0, 50.0],
        },
        Detection {
            class_id: 0, // person, not bird
            confidence: 0.90,
            bbox_xyxy: [10.0, 10.0, 50.0, 50.0],
        },
    ];
    let crop = select_and_pad_bird_crop(&img, &detections, 0.25, (224, 224));
    assert!(
        crop.is_none(),
        "Below threshold or non-bird must return None"
    );
}

#[test]
fn test_detector_select_max_conf_x_area() {
    let img = DynamicImage::ImageRgb8(RgbImage::new(200, 200));
    let detections = vec![
        // Box 1: area = 20 * 20 = 400, conf = 0.8 -> score = 320
        Detection {
            class_id: 14,
            confidence: 0.8,
            bbox_xyxy: [10.0, 10.0, 30.0, 30.0],
        },
        // Box 2: area = 50 * 50 = 2500, conf = 0.5 -> score = 1250 (wins!)
        Detection {
            class_id: 14,
            confidence: 0.5,
            bbox_xyxy: [50.0, 50.0, 100.0, 100.0],
        },
    ];
    let (cropped, bbox, conf) =
        select_and_pad_bird_crop(&img, &detections, 0.25, (224, 224)).expect("Should pick winner");

    assert_eq!(conf, 0.5);
    assert_eq!(cropped.width(), 224);
    assert_eq!(cropped.height(), 224);
    assert_eq!(bbox, [50.0, 50.0, 100.0, 100.0]);
}

#[test]
fn test_detector_15_percent_padding_at_borders_clamped_no_panic() {
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(100, 100, |x, y| {
        if x < 20 && y < 20 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 255, 0])
        }
    }));

    // Box touches left and top borders: x1=0, y1=0
    let detections = vec![Detection {
        class_id: 14,
        confidence: 0.9,
        bbox_xyxy: [0.0, 0.0, 50.0, 50.0],
    }];

    // Width = 50, Height = 50
    // Pad = 7.5
    // Clamped x1 = 0, y1 = 0, x2 = 57.5, y2 = 57.5
    let (cropped, _, _) = select_and_pad_bird_crop(&img, &detections, 0.25, (224, 224))
        .expect("Should crop with clamp without panic");
    assert_eq!(cropped.width(), 224);
    assert_eq!(cropped.height(), 224);

    // Box touches right and bottom borders: x2=100, y2=100
    let detections2 = vec![Detection {
        class_id: 14,
        confidence: 0.9,
        bbox_xyxy: [50.0, 50.0, 100.0, 100.0],
    }];
    let (cropped2, _, _) = select_and_pad_bird_crop(&img, &detections2, 0.25, (224, 224))
        .expect("Should crop with clamp at right bottom");
    assert_eq!(cropped2.width(), 224);
    assert_eq!(cropped2.height(), 224);
}

#[test]
fn test_mock_detector_trait() {
    let detector = MockBirdDetector::new(vec![Detection {
        class_id: 14,
        confidence: 0.95,
        bbox_xyxy: [10.0, 10.0, 90.0, 90.0],
    }]);

    let img = DynamicImage::ImageRgb8(RgbImage::new(100, 100));
    let dets = detector.detect(&img).expect("detect failed");
    assert_eq!(dets.len(), 1);
    assert_eq!(dets[0].confidence, 0.95);
}
