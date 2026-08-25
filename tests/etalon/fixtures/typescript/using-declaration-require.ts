declare function getResource(): { [Symbol.dispose](): void };

using resource = getResource();

require("fs");
