class Foo {
  bar!: string;

  constructor() {
    require(this.bar);
  }
}
