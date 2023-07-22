use html5ever::{
    parse_document,
    serialize::{self, SerializeOpts},
    tendril::TendrilSink,
    tree_builder::TreeBuilderOpts,
    Attribute, ParseOpts,
};
use markup5ever::{
    interface::TreeSink, local_name, namespace_url, ns, serialize::TraversalScope, QualName,
};
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

    script
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

fn main() {
    let source_html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();

    let mut rcdom = parse_html(source_html).unwrap();
    append_script_tag(&mut rcdom, "test.js");

    let result = serialize(&mut rcdom);
}

#[cfg(test)]
mod tests {
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

        assert_eq!(expected_html, result);
    }
}
