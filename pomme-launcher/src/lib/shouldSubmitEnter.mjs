export function shouldSubmitEnter({ key, nativeEvent }) {
  return key === "Enter" && !nativeEvent.isComposing && nativeEvent.keyCode !== 229;
}
