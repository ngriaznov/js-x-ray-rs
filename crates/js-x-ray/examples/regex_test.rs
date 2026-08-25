fn main() {
    use regex_syntax::ast::parse::Parser;
    for p in ["(", "(?=foo)bar", "a{2,1}", "\\", "[", "(?<name>x)", "a**", "(?<=x)y", "\\1"] {
        let r = Parser::new().parse(p);
        match r {
            Ok(_) => println!("{p:?}: OK"),
            Err(e) => println!("{p:?}: ERR kind={:?}", e.kind()),
        }
    }
}
