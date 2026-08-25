declare function Component(options: unknown): ClassDecorator;

@Component({ template: eval("<div></div>") })
class Bar {}
