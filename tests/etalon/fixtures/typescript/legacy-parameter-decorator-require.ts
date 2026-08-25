declare function Inject(token: string): ParameterDecorator;

class Foo {
  constructor(@Inject("TOKEN") private token: string) {
    require(token);
  }
}
