export function ClipboardHistory() {
  return (
    <div>
      <h2 className="text-sm font-medium text-slate-400 mb-4">剪贴板历史</h2>
      <div className="text-center py-12">
        <p className="text-slate-500">暂无同步记录</p>
        <p className="text-xs text-slate-600 mt-2">
          同步过的文本和图片将显示在这里
        </p>
      </div>
    </div>
  );
}
