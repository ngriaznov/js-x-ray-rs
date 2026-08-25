namespace NS {
  enum Mode {
    Safe = "fs",
    Danger = "child_process",
  }

  export function load() {
    return require(Mode.Danger);
  }
}
