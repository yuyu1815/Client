export function shouldSubmitEnter(event: {
  key: string;
  nativeEvent: Pick<KeyboardEvent, "isComposing" | "keyCode">;
}): boolean;
