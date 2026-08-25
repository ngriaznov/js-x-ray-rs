namespace Outer {
  namespace Inner {
    export function run(code: string) {
      return eval(code);
    }
  }
}
