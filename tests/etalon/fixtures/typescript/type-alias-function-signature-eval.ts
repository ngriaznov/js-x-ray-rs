type Runner = (code: string) => unknown;

const run: Runner = (code) => eval(code);

run("this");
