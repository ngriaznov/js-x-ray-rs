class Config {
  constructor(private path: string) {
    require(this.path);
  }
}
