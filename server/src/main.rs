use std::{
    borrow::{Borrow, BorrowMut, Cow},
    rc::Rc,
};

use html5ever::{
    parse_document,
    serialize::{self, SerializeOpts},
    tendril::TendrilSink,
    tree_builder::TreeBuilderOpts,
    Attribute, ParseOpts,
};
use markup5ever::{
    interface::{ElementFlags, NodeOrText, TreeSink},
    local_name, namespace_url, ns,
    serialize::TraversalScope,
    QualName,
};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom, SerializableHandle};

#[derive(Debug)]
struct Working {
    pub is_head: bool,
    pub has_script: bool,

    pub head: Option<Handle>,
    pub last_head_child: Option<Handle>,
    pub last_script: Option<Handle>,
}

impl Default for Working {
    fn default() -> Self {
        Self {
            is_head: false,
            has_script: false,

            head: Default::default(),
            last_head_child: Default::default(),
            last_script: Default::default(),
        }
    }
}

fn walk(handle: &Handle, working: &mut Working) {
    if let NodeData::Element {
        ref name,
        ref attrs,
        ..
    } = handle.data
    {
        match name.local.as_ref() {
            "head" => {
                eprintln!("head ==>");
                working.is_head = true;
                working.head = Some(handle.clone());
            }
            "script" => {
                eprintln!("script!");
                working.has_script = true;

                let parent = handle.parent.take().unwrap();
                let p = parent.upgrade().unwrap();
                let p3 = p.as_ref();
                match p3.data {
                    NodeData::Element { ref name, .. } => {
                        println!("{}??", name.local.as_ref());
                        if working.is_head && name.local.as_ref() == "head" {
                            println!("script element copied.");
                            working.last_script = Some(handle.clone());
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                println!("{}", name.local.as_ref());
            }
        }
    }

    let children = handle.children.borrow();
    for (i, child) in children.iter().enumerate() {
        match handle.data {
            NodeData::Element { ref name, .. } => {
                println!(
                    "{}/{}: {}",
                    i,
                    handle.children.borrow().len() - 1,
                    name.local.as_ref()
                );

                if working.is_head {
                    working.last_head_child = Some(handle.clone());
                }
            }
            _ => {}
        };

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
    let script: Handle = Node::new(NodeData::Element {
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
    });

    let mut a = vec![];
    let document: SerializableHandle = script.clone().into();
    let opt = SerializeOpts {
        scripting_enabled: true,
        traversal_scope: TraversalScope::IncludeNode,
        create_missing_parent: true,
    };
    serialize::serialize(&mut a, &document, opt).ok();
    let result = String::from_utf8_lossy(&a);
    println!("{}", result);

    script
}

fn append_script_tag(rcdom: &mut RcDom, path: &str) {
    let mut working: Working = Default::default();
    walk(&rcdom.get_document(), &mut working);

    let script = create_script(path);

    if working.has_script {
        let element = &working.head.unwrap();
        // let element = &working.last_script.unwrap();
        // let element = &working.last_script.unwrap();
        println!("* has script:");
        print_element(&element);
        rcdom.append(&element, html5ever::tree_builder::AppendNode(script));
    } else {
        // let element = &working.last_head_child.unwrap();
        let element = &working.head.unwrap();
        println!("* has not script:");
        print_element(&element);
        rcdom.append(&element, html5ever::tree_builder::AppendNode(script));
    }
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

fn main() {
    let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();

    let mut rcdom = parse_html(source_html).unwrap();
    append_script_tag(&mut rcdom, "test.js");

    let result = serialize(&mut rcdom);

    println!("{}", result);
    println!("----------------------------------------");
}

#[cfg(test)]
mod tests {
    use html5ever::{
        parse_document, tendril::TendrilSink, tree_builder::TreeBuilderOpts, Attribute, ParseOpts,
    };
    use markup5ever_rcdom::{Handle, NodeData, RcDom};

    use crate::{append_script_tag, parse_html, serialize};

    #[test]
    fn test_1() {
        let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();
        let expected_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script type="text/javascript" src="./test.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();

        let mut rcdom = parse_html(source_html).unwrap();
        let path = "./test.js";
        append_script_tag(&mut rcdom, path);
        let result = serialize(&mut rcdom);

        assert_eq!(expected_html, result);
    }

    #[test]
    fn test_2() {
        let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script src="already_script.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();
        let expected_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title><script src="already_script.js"></script><script type="text/javascript" src="./test.js"></script></head><body><h1>hello</h1></body></html>"#.to_owned();

        let mut rcdom = parse_html(source_html).unwrap();
        let path = "./test.js";
        append_script_tag(&mut rcdom, path);
        let result = serialize(&mut rcdom);

        println!("{}", result);

        assert_eq!(expected_html, result);
    }
}
