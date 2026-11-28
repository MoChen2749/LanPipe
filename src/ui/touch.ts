/**
 * 触摸手势助手（仅 Android 分支使用）。
 */

/**
 * 长按识别：pointerdown 后按住 duration 毫秒触发，
 * 移动超过 8px、抬起或取消都会取消识别。返回解绑函数。
 */
export function longPress(node: HTMLElement, handler: () => void, duration = 480): () => void {
  let timer = 0;
  let startX = 0;
  let startY = 0;

  const cancel = () => {
    if (timer) window.clearTimeout(timer);
    timer = 0;
  };

  const onDown = (event: PointerEvent) => {
    if (event.pointerType === "mouse") return; // 桌面鼠标走 click
    cancel();
    startX = event.clientX;
    startY = event.clientY;
    timer = window.setTimeout(() => {
      timer = 0;
      handler();
    }, duration);
  };

  const onMove = (event: PointerEvent) => {
    if (!timer) return;
    if (Math.abs(event.clientX - startX) > 8 || Math.abs(event.clientY - startY) > 8) cancel();
  };

  node.addEventListener("pointerdown", onDown);
  node.addEventListener("pointermove", onMove);
  node.addEventListener("pointerup", cancel);
  node.addEventListener("pointercancel", cancel);
  node.addEventListener("pointerleave", cancel);
  node.addEventListener("contextmenu", (event) => event.preventDefault());

  return () => {
    cancel();
    node.removeEventListener("pointerdown", onDown);
    node.removeEventListener("pointermove", onMove);
    node.removeEventListener("pointerup", cancel);
    node.removeEventListener("pointercancel", cancel);
    node.removeEventListener("pointerleave", cancel);
  };
}
