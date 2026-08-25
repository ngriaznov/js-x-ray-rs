function run<T>(x: T): T {
  return eval(x as unknown as string);
}

run("this");
