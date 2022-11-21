use actix_web::{
    get, post,
    web::{Bytes, Data},
    App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use futures::Stream;
use opencv::{
    core::Vector,
    imgcodecs,
    prelude::*,
    videoio::{self, VideoCapture},
};
use std::{
    fs,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    thread::{sleep, spawn},
    time::Duration,
};
use tokio::sync::mpsc::{channel, Receiver};

#[post("/start")]
async fn start(state: Data<Arc<State>>, req: HttpRequest) -> impl Responder {
    if let Some(addr) = req.connection_info().peer_addr() {
        println!("start from {}", addr);
    } else {
        println!("start from unkown hacker");
    }

    {
        // 使用 block 在確認後就返還讀寫鎖
        let mut running = {
            let locker = state.running.lock();
            if let Err(err) = locker {
                return format!("running 取鎖失敗 {}", err);
            }
            locker.unwrap()
        };

        if *running {
            return "相機已經開啟".to_string();
        }

        *running = true;

        let mut cam = {
            let cam = state.cam.lock();
            if let Err(err) = cam {
                return format!("cam 取鎖失敗 {}", err);
            }
            cam.unwrap()
        };
        *cam = {
            let is_number = state.cam_id.trim().parse::<i32>().is_ok();
            if is_number {
                Some(videoio::VideoCapture::new(0, videoio::CAP_ANY).unwrap())
            } else {
                Some(videoio::VideoCapture::from_file(&state.cam_id, videoio::CAP_ANY).unwrap())
            }
        };
        if let Some(ccc) = &mut *cam {
            if let Err(err) = ccc.set(videoio::CAP_PROP_BUFFERSIZE, 2.0) {
                return format!("cam 設定參數失敗 {}", err);
            }
        }
    };

    spawn(move || {
        loop {
            // println!("開始+1");
            {
                let running = {
                    let locker = state.running.lock();
                    if let Err(err) = locker {
                        return println!("running 取鎖失敗 {}", err);
                    }
                    locker.unwrap()
                };

                if !*running {
                    return println!("已經停止，結束迴圈");
                }

                let mut cam = {
                    let locker = state.cam.lock();
                    if let Err(err) = locker {
                        return println!("cam 取鎖失敗 {}", err);
                    }
                    locker.unwrap()
                };

                let mut frame = {
                    let locker = state.frame.lock();
                    if let Err(err) = locker {
                        return println!("frame 取鎖失敗 {}", err);
                    }
                    locker.unwrap()
                };

                if let Some(cam2) = &mut *cam {
                    if let Some(err) = cam2.read(&mut *frame).err() {
                        return println!("相機讀取失敗，結束迴圈: {}", err);
                    }
                } else {
                    return println!("相機已經關閉，結束迴圈");
                }

                // let mut counter = {
                //     let locker = state.counter.lock(); // <- go RWMutex.Lock()
                //     if let Err(err) = locker {
                //         return println!("counting 取鎖失敗 {}", err);
                //     }
                //     locker.unwrap()
                // };
                // *counter += 1; // 開始對資料做存取
            }
            sleep(Duration::from_millis(1));
        }
    });

    "開始相機".to_string()
}

#[post("/stop")]
async fn stop(state: Data<Arc<State>>, req: HttpRequest) -> impl Responder {
    if let Some(addr) = req.connection_info().peer_addr() {
        println!("stop from {}", addr);
    } else {
        println!("stop from unkown hacker");
    }

    {
        let mut running = state.running.lock().unwrap();
        if !*running {
            return "相機已經關閉".to_string();
        }

        *running = false;
        let mut cam = {
            let locker = state.cam.lock();
            if let Err(err) = locker {
                return format!("cam 取鎖失敗 {}", err);
            }
            locker.unwrap()
        };

        if let Some(cam2) = &mut *cam {
            cam2.release().unwrap();
            *cam = None
        }
    }

    "關閉相機".to_string()
}

fn get_image_payload(img: Vec<u8>) -> Bytes {
    let mut msg = format!(
        "--boundarydonotcross\r\nContent-Length:{}\r\nContent-Type:image/jpeg\r\n\r\n",
        img.len()
    )
    .into_bytes();
    msg.extend(img);
    Bytes::from([msg].concat())
}

// wrap Receiver in own type, with correct error type
struct Client(Receiver<Bytes>);

impl Stream for Client {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.0).poll_recv(cx) {
            Poll::Ready(Some(v)) => Poll::Ready(Some(Ok(v))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[get("/live")]
async fn live(state: Data<Arc<State>>, req: HttpRequest) -> impl Responder {
    if let Some(addr) = req.connection_info().peer_addr() {
        println!("live from {}", addr);
    } else {
        println!("live from unkown hacker");
    }

    let (tx, rx) = channel(100);
    let client = Client(rx);
    let mut res = HttpResponse::Ok();

    {
        let mut buf: Vector<u8> = Vector::new();
        let running = state.running.lock().unwrap();
        if !*running {
            return res.body("請先打開相機");
        }

        let frame = {
            let locker = state.frame.lock();
            if let Err(err) = locker {
                return res.body(format!("frame 取鎖失敗 {}", err));
            }
            locker.unwrap()
        };

        let params = Vector::new();
        imgcodecs::imencode(".jpg", &*frame, &mut buf, &params).unwrap();
        tx.send(get_image_payload(buf.to_vec())).await.unwrap();
    }

    tokio::spawn(async move {
        loop {
            {
                let running = state.running.lock().unwrap();
                if !*running {
                    println!("請先打開相機");
                    return;
                }
            }

            let mut buf: Vector<u8> = Vector::new();
            {
                let frame = {
                    let locker = state.frame.lock();
                    if let Err(err) = locker {
                        println!("frame 取鎖失敗 {}", err);
                        return;
                    }
                    locker.unwrap()
                };

                if frame.empty() {
                    println!("frame 目前沒影像，等下一秒");
                    // 有鎖(MutexGuard)的地方不能使用await，所以只能用標準sleep
                    sleep(Duration::from_secs(1));
                    continue;
                }

                let params = Vector::new();
                imgcodecs::imencode(".jpg", &*frame, &mut buf, &params).unwrap();
            }

            if let Err(err) = tx.try_send(get_image_payload(buf.to_vec())) {
                println!("channel 傳送失敗，使用者可能離開: {}", err);
                return;
            };

            // sleep 使用 tokio的sleep，才不會導致串流卡住
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    res.content_type("multipart/x-mixed-replace;boundary=boundarydonotcross")
        .streaming(client)
}

struct State {
    cam_id: String,
    cam: Mutex<Option<VideoCapture>>,
    frame: Mutex<Mat>,
    running: Mutex<bool>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    std::env::set_var("RUST_BACKTRACE", "1");
    env_logger::init();

    let cam_id = fs::read_to_string("cam.conf").unwrap();
    let state = Arc::new(State {
        cam_id,
        cam: Mutex::new(None),
        frame: Mutex::new(Mat::default()),
        running: Mutex::new(false),
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    println!("Listening {}", addr);

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .service(start)
            .service(stop)
            .service(live)
    })
    .bind(addr)?
    .run()
    .await
}
