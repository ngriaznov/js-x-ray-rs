function load(x: string): string;
function load(x: number): number;
function load(x: any): any {
  return require(x);
}

load("fs");
