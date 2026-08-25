abstract class Base {
  abstract run(): void;
}

class Impl extends Base {
  run() {
    require("fs");
  }
}

new Impl().run();
