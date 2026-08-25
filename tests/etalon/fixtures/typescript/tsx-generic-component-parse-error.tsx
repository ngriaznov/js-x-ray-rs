interface Props<T> {
  value: T;
}

function Box<T>(props: Props<T>) {
  return <div>{require(String(props.value))}</div>;
}
