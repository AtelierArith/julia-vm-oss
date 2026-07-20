use subset_julia_vm_parser::parse;

fn print_tree(node: &subset_julia_vm_parser::cst::CstNode, source: &str, indent: usize) {
    let prefix = " ".repeat(indent);
    let text = node.text_from_source(source);
    let text_short = if text.len() > 30 {
        format!("{}...", &text[..30])
    } else {
        text.to_string()
    };
    // Print kind using as_str() not Debug
    println!(
        "{}{} [{}-{}] text={:?}",
        prefix,
        node.kind.as_str(),
        node.span.start,
        node.span.end,
        text_short
    );
    for child in &node.children {
        print_tree(child, source, indent + 2);
    }
}

fn main() {
    let source = "neg_int(x) = Core.Intrinsics.neg_int(x)";
    match parse(source) {
        Ok(cst) => {
            println!("Source: {:?}\n", source);
            print_tree(&cst, source, 0);
        }
        Err(e) => println!("Parse error: {}", e),
    }
}
