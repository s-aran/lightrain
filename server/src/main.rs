use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_files::NamedFile;
use actix_web::dev::Server;
use actix_web::http::header::{ContentDisposition, DispositionType};
use actix_web::{
    get, middleware, route, web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
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
use serde::{Deserialize, Serialize};

#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

#[derive(Message)]
#[rtype(usize)]
pub struct Connect {
    pub addr: Recipient<Message>,
}

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

#[derive(Serialize, Deserialize, Debug)]
enum Command {
    NoOperation,

    Hello { type_name: String },
    Reload,

    Ping,

    Echo { id: u64, message: String },
}

#[derive(Serialize, Deserialize, Debug)]
enum CommandResult {
    NoOperation,

    Hello {
        id: u64,
    },
    Reload,

    Ping,

    Echo {
        id: u64,
        from_id: u64,
        message: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct ControllerCommandRequest {
    id: u64,
    controller_id: u32,
    from_ip_address: String,
    from_port_number: u32,
    command: Command,
}

#[derive(Serialize, Deserialize, Debug)]
struct ControllerCommandResponse {
    id: u64,
    controller_id: u32,
    from_ip_address: String,
    from_port_number: u32,
    result: CommandResult,
}

#[derive(Serialize, Deserialize, Debug)]
struct ClientCommandRequest {
    id: u64,
    client_id: u32,
    from_ip_address: String,
    from_port_number: u32,
    from_controller_id: u64,
    command: Command,
}

#[derive(Serialize, Deserialize, Debug)]
struct ClientCommandResponse {
    id: u64,
    client_id: u32,
    from_ip_address: String,
    from_port_number: u32,
    from_controller_id: u64,
    result: CommandResult,
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

    res
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

impl Handler<Connect> for LightrainWebsocketServer {
    type Result = usize;

    fn handle(&mut self, msg: Connect, ctx: &mut Self::Context) -> Self::Result {
        println!("connect");

        0
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
            Ok(ws::Message::Text(text)) => {
                // let request = match serde_json::from_str(&text) {
                //     Ok(j) => {}
                //     Err(e) => {
                //         eprintln!("Error: {e}: {text}");
                //         return ctx.text("");
                //     }
                // };

                // println!("??: {:?}", item.unwrap());

                return ctx.text(text);
            }
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

#[get("/")]
async fn index(request: HttpRequest) -> impl Responder {
    let filename = "index.html";
    let filepath = Path::new(filename);
    let response = make_response_from_file(filepath, Some(true));

    HttpResponse::Ok().body(response)
}

#[get("/favicon.ico")]
async fn favicon() -> impl Responder {
    HttpResponse::Ok().content_type("image/x-icon").body("")
}

#[get("/{filename}")]
async fn others(request: HttpRequest) -> impl Responder {
    let filename = request.match_info().get("filename").unwrap();

    println!("{}", filename);

    let filepath = Path::new(filename);
    let response = make_response_from_file(filepath, Some(true));

    HttpResponse::Ok().body(response)
}

#[get("/script/{filename:.*\\.js}")]
async fn script_index(request: HttpRequest) -> Result<NamedFile, Error> {
    let base = Path::new("script");
    let path: PathBuf = request.match_info().query("filename").parse().unwrap();
    let file = NamedFile::open(base.join(path))?;
    Ok(file
        .use_last_modified(true)
        .set_content_disposition(ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![],
        }))
}

#[route("/**lightrain_controller**/", method = "GET")]
async fn echo_ws(request: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    ws::start(LightrainWebsocketServer::new(), &request, stream)
}

fn run_server(bind: &String, workers: usize) -> Server {
    HttpServer::new(|| {
        App::new()
            .service(index)
            .service(favicon)
            .service(others)
            .service(script_index)
            // .service(Files::new("/", ".").index_file("index.html"))
            // .service(web::resource("/**lightrain_controller**/").route(web::get().to(echo_ws)))
            .service(echo_ws)
            .wrap(middleware::Logger::default())
    })
    .workers(workers)
    .bind(bind)
    .unwrap()
    .run()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    run_server(&"127.0.0.1:5776".to_owned(), 3).await
}

#[cfg(test)]
mod tests {
    use actix_web::App;

    use crate::{append_script_tag, echo_ws, parse_html, serialize};

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

    // async fn _send_by_ws(target: &str, msg: &str) -> String {
    //     let (_, mut conn) = match awc::Client::new().ws(target).connect().await {
    //         Ok(result) => result,
    //         Err(err) => {
    //             eprintln!("Error: {}", err.to_string());
    //             return "".to_owned();
    //         }
    //     };

    //     conn.send(ws::Message::Text(msg.into())).await.unwrap();
    //     while let Some(Ok(Frame::Text(response))) = conn.next().await {
    //         return String::from_utf8_lossy(&response).to_string();
    //     }

    //     "".to_owned()
    // }

    // async fn send_by_ws(target: &str, msg: &str) -> String {
    //     let a = _send_by_ws(target, msg);
    //     join!(a).0
    // }

    #[actix_rt::test]
    async fn test_ws_1() {
        let addr = "127.0.0.1:15776";
        let _url = format!("ws://{}/**lightrain_controller**/", addr);
        let url = _url.as_str();

        // let app = actix_web::test::init_service(App::new().service(echo_ws)).await;
        let app = actix_web::test::init_service(App::new().service(echo_ws)).await;
        let req = actix_web::test::TestRequest::default()
            .set_payload("Client Hello".as_bytes())
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        print!("{:?}", res);
        assert!(false);
    }
}
