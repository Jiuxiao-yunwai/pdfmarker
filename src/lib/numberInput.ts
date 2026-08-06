export function stepNumberInput(event: WheelEvent) {
  const input = event.currentTarget;
  if (!(input instanceof HTMLInputElement) || input.disabled || input.readOnly || event.deltaY === 0) return;

  event.preventDefault();
  if (event.deltaY < 0) input.stepUp();
  else input.stepDown();

  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}
