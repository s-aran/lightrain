use std::{
    borrow::{Borrow, BorrowMut},
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
    interface::{NodeOrText, TreeSink},
    local_name, namespace_url, ns,
    serialize::TraversalScope,
    QualName,
};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom, SerializableHandle};

#[derive(Debug)]
struct Working {
    pub is_meta: bool,
    pub has_script: bool,

    pub meta: Option<Handle>,
}

impl Default for Working {
    fn default() -> Self {
        Self {
            is_meta: false,
            has_script: false,

            meta: Default::default(),
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
        println!("{}", name.local.as_ref());

        match name.local.as_ref() {
            "meta" => {
                eprintln!("meta ==>");
                working.is_meta = true;
            }
            "script" => {
                eprintln!("script!");
                working.has_script = true;
            }
            _ => {
                if working.is_meta {
                    working.meta = Some(handle.clone());
                    working.is_meta = false;
                    println!("<== meta");
                }
            }
        }
    }

    let children = handle.children.borrow();
    for (i, child) in children.iter().enumerate() {
        walk(child, working);
    }
}

fn create_script() -> Handle {
    let script: Handle = Node::new(NodeData::Element {
        name: QualName::new(None, ns!(html), local_name!("script")),
        attrs: vec![
            Attribute {
                name: QualName::new(None, ns!(), local_name!("type")),
                value: "text/javascript".into(),
            },
            Attribute {
                name: QualName::new(None, ns!(), local_name!("src")),
                value: "test.js".into(),
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

fn main() {
    let source_html = r#"<!DOCTYPE html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();

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

    let mut rcdom = parse_document(rcdom_sink, opts)
        .from_utf8()
        .read_from(&mut a)
        .unwrap();

    let mut working: Working = Default::default();
    walk(&rcdom.get_document(), &mut working);

    let script = create_script();
    rcdom.append_before_sibling(
        &working.meta.unwrap(),
        html5ever::tree_builder::AppendNode(script),
    );

    let document: SerializableHandle = rcdom.get_document().into();
    let mut bytes = vec![];
    serialize::serialize(&mut bytes, &document, Default::default())
        .ok()
        .expect("serialization failed");
    let result = String::from_utf8_lossy(&bytes);
    println!("{}", result);
    println!("----------------------------------------");
}

#[cfg(test)]
mod tests {
    use html5ever::{
        parse_document, tendril::TendrilSink, tree_builder::TreeBuilderOpts, Attribute, ParseOpts,
    };
    use markup5ever_rcdom::{Handle, NodeData, RcDom};

    #[test]
    fn test_1() {
        let source_html = r#"<!DOCTYPE html><head><meta charset="utf-8"><title>test</title></head><body><h1>hello</h1></body></html>"#.to_owned();

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

        let rcdom = parse_document(rcdom_sink, opts)
            .from_utf8()
            .read_from(&mut a)
            .unwrap();

        let mut working: Working = Default::default();
        walk(&rcdom.document, &mut working);

        assert!(false)
    }

    #[derive(Debug)]
    struct Working {
        pub is_meta: bool,
    }

    impl Default for Working {
        fn default() -> Self {
            Self { is_meta: false }
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
                "meta" => {
                    eprintln!("meta ==>");
                    working.is_meta = true;
                }
                _ => {
                    if working.is_meta {
                        working.is_meta = false;
                        println!("<== meta");
                    }
                }
            }
        }
    }
}
