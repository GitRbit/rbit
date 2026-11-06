export function sampleCounts() {
  const counts = { samples: 0, compileComparable: 0, compileInvalid: 0, memoryComparable: 0, memoryInvalid: 0 };
  return {
    counts,
    observe(record) {
      if (record.kind !== "sample") return;
      counts.samples++;
      counts[record.validity?.compileComparable ? "compileComparable" : "compileInvalid"]++;
      counts[record.validity?.memoryComparable ? "memoryComparable" : "memoryInvalid"]++;
    },
  };
}
