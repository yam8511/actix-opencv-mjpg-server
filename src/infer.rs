use opencv::{
    core::{self, Rect, Scalar},
    dnn, imgcodecs, imgproc,
    prelude::*,
};

fn draw_bounding_box(
    img: &mut Mat,
    name: &str,
    confidence: f32,
    rect: Rect,
    color: Scalar,
    font_scale: f64,
    thickness: i32,
    font_face: i32,
) {
    let label = format!("{} ({:.2}%)", name, confidence * 100.0);
    let label_size =
        imgproc::get_text_size(&label, font_face, font_scale, thickness, &mut 0).unwrap();
    let x1 = rect.x;
    let y1 = rect.y;
    // let x2 = x1 + label_size.width;
    // let y2 = y1 - label_size.height;

    imgproc::rectangle(img, rect, color, thickness, imgproc::LINE_8, 0).unwrap();
    imgproc::rectangle(
        img,
        Rect::new(x1, y1, label_size.width, label_size.height),
        color,
        thickness,
        imgproc::LINE_8,
        0,
    )
    .unwrap();
    imgproc::put_text(
        img,
        &label,
        core::Point::new(rect.x, rect.y),
        font_face,
        font_scale,
        Scalar::new(255.0, 255.0, 255.0, 0.0),
        thickness,
        imgproc::LINE_8,
        false,
    )
    .unwrap();
}

fn main() {
    // 初始化模型
    let mut net = dnn::read_net_from_onnx("yolov8n.onnx").unwrap();

    // 设置推理后端为默认后端（CPU）
    net.set_preferable_backend(dnn::DNN_BACKEND_CUDA).unwrap();

    // 设置推理目标为默认目标（CPU）
    net.set_preferable_target(dnn::DNN_TARGET_CUDA).unwrap();

    // 图像尺寸
    let img_size = core::Size::new(640, 640);

    for _ in 0..10 {
        // 读取图像
        let mut img = imgcodecs::imread("bus.jpg", imgcodecs::IMREAD_UNCHANGED).unwrap();

        let now = std::time::Instant::now();

        let x_factor = img.size().unwrap().width as f32 / img_size.width as f32;
        let y_factor = img.size().unwrap().height as f32 / img_size.height as f32;

        // 图像预处理
        let mut blob = dnn::blob_from_image(
            &img,
            1.0 / 255.0,
            img_size,
            core::Scalar::new(0.0, 0.0, 0.0, 0.0),
            true,
            false,
            core::CV_32F,
        )
        .unwrap();
        let mean = Scalar::new(0., 0., 0., 0.);
        net.set_input(&mut blob, "images", 1.0, mean).unwrap();
        let pre_p = now.elapsed();
        let now = std::time::Instant::now();

        // 前向推理
        let output = net.forward_single("output0").unwrap();

        let infer_p = now.elapsed();
        let now = std::time::Instant::now();

        // 后处理
        let dims = output.size().unwrap().width;

        let output = output.reshape(1, dims).unwrap();
        let output = output.t().unwrap().to_mat().unwrap();

        let rows = output.size().unwrap().height;

        // println!("row = {}", rows);
        // println!("dim = {}", dims);

        let mut boxes = core::Vector::new();
        let mut scores = core::Vector::new();
        let mut class_ids = Vec::new();

        for i in 0..rows {
            let row = output
                .row_range(&core::Range::new(i, i + 1).unwrap())
                .unwrap();
            let col = row.col_range(&core::Range::new(4, dims).unwrap()).unwrap();

            let mut max_val = 0.;
            let max_score = Some(&mut max_val);
            let mut loc = core::Point::new(0, 0);
            let max_loc = Some(&mut loc);
            core::min_max_loc(&col, None, max_score, None, max_loc, &core::no_array()).unwrap();

            // let cx = row.;
            let cx = row.at::<f32>(0).unwrap();
            let cy = row.at::<f32>(1).unwrap();
            let w = row.at::<f32>(2).unwrap();
            let h = row.at::<f32>(3).unwrap();
            let x = cx - w * 0.5;
            let y = cy - h * 0.5;
            let x2 = x + w;
            let y2 = y + h;

            let rect = Rect::new(
                x.round() as i32,
                y.round() as i32,
                (x2.round() - x.round()) as i32,
                (y2.round() - y.round()) as i32,
            );

            boxes.push(rect);
            scores.push(max_val as f32);
            class_ids.push(loc.x);
        }

        let mut indices: core::Vector<i32> = core::Vector::new();
        dnn::nms_boxes(&boxes, &scores, 0.8, 0.5, &mut indices, 1.0, 0).unwrap();
        let post_p = now.elapsed();
        println!(
            "{} pre-process, {} inference, {} post-process, total {}",
            pre_p.as_millis(),
            infer_p.as_millis(),
            post_p.as_millis(),
            (pre_p + infer_p + post_p).as_millis()
        );

        for idx in indices {
            let idx = idx as usize;

            if idx == 0 {
                continue;
            }

            let box_rect = boxes.get(idx).unwrap();
            let class_id = class_ids[idx];

            draw_bounding_box(
                &mut img,
                &class_id.to_string(),
                scores.get(idx).unwrap(),
                Rect::new(
                    (box_rect.x as f64 * x_factor as f64).round() as i32,
                    (box_rect.y as f64 * y_factor as f64).round() as i32,
                    (box_rect.width as f64 * x_factor as f64).round() as i32,
                    (box_rect.height as f64 * y_factor as f64).round() as i32,
                ),
                core::Scalar::new(0.0, 0.0, 255.0, 0.0),
                1.0,
                3,
                imgproc::FONT_HERSHEY_SIMPLEX,
            );
        }

        // 显示结果图像
        imgcodecs::imwrite("result.jpg", &img, &core::Vector::new()).unwrap();
        // highgui::imshow("Result", &img).unwrap();
        // highgui::wait_key(0).unwrap();
    }
}
