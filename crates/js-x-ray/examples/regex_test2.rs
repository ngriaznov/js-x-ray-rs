fn main() {
    use js_x_ray::utils::safe_regex::is_safe_regex;
    for p in ["(?=(a+)+)b", "(?:(a+)+)b", "(a+)+b", "(?<=(a+)+)b", "(a+){10}", "(?=foo)bar"] {
        println!("{p:?} -> safe: {}", is_safe_regex(p));
    }
}
