use rmudos::compiler::preprocess;
use std::path::PathBuf;
fn main() {
  let root = PathBuf::from("mudlib");
  let path = root.join("std/living/combat.c");
  let src = std::fs::read_to_string(&path).unwrap();
  match preprocess::preprocess(&src, &path, &root) {
    Ok(out) => {
      for (i, line) in out.lines().enumerate() {
        if i < 45 {
          println!("{:>3}|{}", i+1, line);
        }
      }
    }
    Err(e) => eprintln!("{e:#}"),
  }
}
