type Kinded = { kind: string }

export type KindHandlers<Union extends Kinded, Args extends unknown[], Result> = {
  [Kind in Union['kind']]: (value: Extract<Union, { kind: Kind }>, ...args: Args) => Result
}

export function dispatchByKind<Union extends Kinded, Args extends unknown[], Result>(
  handlers: KindHandlers<Union, Args, Result>,
  value: Union,
  ...args: Args
): Result {
  const handler = handlers[value.kind as Union['kind']] as (value: Union, ...args: Args) => Result
  return handler(value, ...args)
}
