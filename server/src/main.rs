use std::fs::File;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

use html5ever::{
    parse_document,
    serialize::{self},
    tendril::TendrilSink,
    tree_builder::TreeBuilderOpts,
    Attribute, ParseOpts,
};

use markup5ever::{interface::TreeSink, local_name, namespace_url, ns, QualName};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom, SerializableHandle};

#[derive(Debug)]
struct Working {
    pub is_head: bool,
    pub head: Option<Handle>,
}

impl Default for Working {
    fn default() -> Self {
        Self {
            is_head: false,
            head: Default::default(),
        }
    }
}

fn walk(handle: &Handle, working: &mut Working) {
    if let NodeData::Element { ref name, .. } = handle.data {
        match name.local.as_ref() {
            "head" => {
                working.is_head = true;
                working.head = Some(handle.clone());
            }
            _ => {}
        }
    }

    let children = handle.children.borrow();
    for child in children.iter() {
        walk(child, working);
    }

    match handle.data {
        NodeData::Element { ref name, .. } => {
            if name.local.as_ref() == "head" {
                working.is_head = false;
            }
        }
        _ => {}
    }
}

fn create_script(path: &str) -> Handle {
    Node::new(NodeData::Element {
        name: QualName::new(None, ns!(html), local_name!("script")),
        attrs: vec![
            Attribute {
                name: QualName::new(None, ns!(), local_name!("type")),
                value: "text/javascript".into(),
            },
            Attribute {
                name: QualName::new(None, ns!(), local_name!("src")),
                value: path.into(),
            },
        ]
        .into(),
        template_contents: None.into(),
        mathml_annotation_xml_integration_point: false,
    })
}

fn append_script_tag(rcdom: &mut RcDom, path: &str) {
    let mut working: Working = Default::default();
    walk(&rcdom.get_document(), &mut working);

    let script = create_script(path);
    let element = &working.head.unwrap();
    print_element(&element);
    rcdom.append(&element, html5ever::tree_builder::AppendNode(script));
}

fn print_element(element: &Handle) {
    match element.data {
        NodeData::Element { ref name, .. } => {
            println!("{}", name.local.as_ref());
        }
        _ => {
            println!("not element");
        }
    }
}

fn parse_html(source_html: String) -> Result<RcDom, std::io::Error> {
    let mut a = source_html.as_bytes();
    let rcdom_sink = RcDom::default();
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            drop_doctype: false,
            ..Default::default()
        },
        ..Default::default()
    };

    parse_document(rcdom_sink, opts)
        .from_utf8()
        .read_from(&mut a)
}

fn serialize(rcdom: &mut RcDom) -> String {
    let document: SerializableHandle = rcdom.get_document().into();
    let mut bytes = vec![];
    serialize::serialize(&mut bytes, &document, Default::default())
        .ok()
        .expect("serialization failed");
    String::from_utf8_lossy(&bytes).to_string()
}

fn inject_script_element(src: String) -> String {
    let mut rcdom = parse_html(src).unwrap();
    append_script_tag(&mut rcdom, "script/lightrain_injection.js");
    serialize(&mut rcdom)
}

fn make_response_from_file(filepath: &Path, injection: Option<bool>) -> String {
    let mut file = File::open(filepath).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    let res = match filepath.extension() {
        Some(ext) => match ext.to_str().unwrap() {
            "html" => match injection {
                Some(true) => inject_script_element(contents),
                _ => contents,
            },
            _ => contents,
        },
        _ => contents,
    };

    format!("HTTP/1.1 200 OK\r\n\r\n{}", res)
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap();

    println!("Request: {}", String::from_utf8_lossy(&buffer[..]));

    let filepath = get_filename_from_get_request(&buffer);
    let path = Path::new(&filepath);
    if path.exists() && path.is_file() {
        let response = make_response_from_file(&path, Some(true));
        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    let index_path = Path::new("index.html");
    if path.exists() && path.is_dir() && index_path.exists() {
        let response = make_response_from_file(&index_path, Some(true));
        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    } else {
        let status_line = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
        let mut file = File::open("404.html").unwrap();
        let mut contents = String::new();

        file.read_to_string(&mut contents).unwrap();

        let response = format!("{}{}", status_line, contents);

        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    }
}

fn get_filename_from_get_request(request: &[u8]) -> String {
    let re = regex::bytes::Regex::new(r"^GET\s+(.+)\s+HTTP/1.1").unwrap();
    let caps = re.captures(request).unwrap();
    let filename_match = caps.get(1).unwrap();
    let filename = filename_match.as_bytes();

    let decoded = urlencoding::decode_binary(filename);
    format!(".{}", String::from_utf8_lossy(&decoded).to_string())
}

fn is_expected_hello(message: &String, expected_type: &str) -> bool {
    let re = regex::Regex::new(r"^Hello (.+)$").unwrap();
    let caps = re.captures(&message).unwrap();
    match caps.get(1) {
        Some(hello) => {
            if hello.as_str() == expected_type {
                return true;
            }
        }
        _ => return false,
    }

    false
}

fn is_client_hello(message: &String) -> bool {
    is_expected_hello(message, "client")
}

fn is_operator_hello(message: &String) -> bool {
    is_expected_hello(message, "operator")
}

pub struct LightrainWebsocketServer {
    hb: Instant,
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

impl LightrainWebsocketServer {
    pub fn new() -> Self {
        Self { hb: Instant::now() }
    }

    fn heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                println!("Websocket Clietn heartbeat failed, disconnecting!");

                ctx.stop();

                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for LightrainWebsocketServer {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for LightrainWebsocketServer {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        println!("WS: {item:?}");

        match item {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => ctx.text(text),
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

async fn echo_ws(request: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    ws::start(LightrainWebsocketServer::new(), &request, stream)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Hello, world!");

    thread::spawn(|| {
        let listener = TcpListener::bind("127.0.0.1:5775").unwrap();
        for s in listener.incoming() {
            thread::spawn(|| {
                let stream = s.unwrap();

                println!("connection established");
                handle_connection(stream);
            });
        }
    });

    HttpServer::new(|| {
        App::new()
            .service(web::resource("/").route(web::get().to(echo_ws)))
            .wrap(middleware::Logger::default())
    })
    .workers(2)
    .bind("127.0.0.1:5776")?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use crate::{append_script_tag, get_filename_from_get_request, parse_html, serialize};

    #[test]
    fn test_append_script_tag_without_script_elements() {
        let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();
        let expected_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script type="text/javascript" src="./test.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();

        let mut rcdom = parse_html(source_html).unwrap();
        let path = "./test.js";
        append_script_tag(&mut rcdom, path);
        let result = serialize(&mut rcdom);

        assert_eq!(expected_html, result);
    }

    #[test]
    fn test_append_script_tag_with_script_element() {
        let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script src="already_script.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();
        let expected_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script src="already_script.js"></script><script type="text/javascript" src="./test.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();

        let mut rcdom = parse_html(source_html).unwrap();
        let path = "./test.js";
        append_script_tag(&mut rcdom, path);
        let result = serialize(&mut rcdom);

        assert_eq!(expected_html, result);
    }

    #[test]
    fn test_get_filename_from_get_request() {
        let request_1 = b"GET /test.html HTTP/1.1\r\n";
        assert_eq!("./test.html", get_filename_from_get_request(request_1));

        let request_2 = b"GET /script/%E3%82%BD%E3%83%BC%E3%82%B9%E3%83%95%E3%82%A1%E3%82%A4%E3%83%AB.js HTTP/1.1\r\n";
        assert_eq!(
            "./script/ソースファイル.js",
            get_filename_from_get_request(request_2)
        );

        let request_3 = b"GET / HTTP/1.1\r\n";
        assert_eq!("./", get_filename_from_get_request(request_3));
    }
}
