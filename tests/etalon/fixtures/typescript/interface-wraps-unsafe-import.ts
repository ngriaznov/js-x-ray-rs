interface Config {
  path: string;
}

function load(cfg: Config) {
  return require(cfg.path);
}

load({ path: "./plugin" });
