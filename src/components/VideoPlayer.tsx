export function VideoPlayer() {
  return (
    <div className="w-full h-full flex flex-col items-center justify-center text-neutral-500">
      <div className="text-6xl mb-4">▶</div>
      <p className="text-lg">No video loaded</p>
      <p className="text-sm mt-2">Open a file to preview</p>
    </div>
  );
}
