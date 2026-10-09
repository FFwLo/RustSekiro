// Decompiles every function of the program to C files for offline indexing (gamedb, rg).
// Args: <out_dir>. One file per 64 KiB of address space: <out_dir>/<addr >> 16>.c, each
// function prefixed with "// @ <entry>". Resumable: buckets whose file exists are skipped.
// @category ShinobiCombat
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.parallel.DecompileConfigurer;
import ghidra.app.decompiler.parallel.DecompilerCallback;
import ghidra.app.decompiler.parallel.ParallelDecompiler;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.util.task.TaskMonitor;

import java.io.File;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

public class DecompileAll extends GhidraScript {
    @Override
    protected void run() throws Exception {
        File out = new File(getScriptArgs()[0]);
        out.mkdirs();
        Map<Long, List<Function>> buckets = new TreeMap<>();
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (f.isThunk() || f.isExternal()) continue;
            buckets.computeIfAbsent(f.getEntryPoint().getOffset() >> 16, k -> new ArrayList<>()).add(f);
        }
        DecompileConfigurer cfg = d -> {
            d.setOptions(new DecompileOptions());
            d.toggleCCode(true);
            d.toggleSyntaxTree(false);
            d.setSimplificationStyle("decompile");
        };
        int done = 0;
        for (Map.Entry<Long, List<Function>> b : buckets.entrySet()) {
            done++;
            File file = new File(out, Long.toHexString(b.getKey()) + ".c");
            if (file.exists()) continue;
            DecompilerCallback<String> cb = new DecompilerCallback<String>(currentProgram, cfg) {
                @Override
                public String process(DecompileResults r, TaskMonitor m) {
                    Function f = r.getFunction();
                    String body = r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage() + "\n";
                    return "// @ " + f.getEntryPoint() + "\n" + body;
                }
            };
            cb.setTimeout(60);
            List<String> parts;
            try {
                parts = ParallelDecompiler.decompileFunctions(cb, b.getValue(), monitor);
            } finally {
                cb.dispose();
            }
            File tmp = new File(out, file.getName() + ".tmp");
            try (PrintWriter w = new PrintWriter(tmp, "UTF-8")) {
                for (String p : parts) w.println(p);
            }
            tmp.renameTo(file);
            if (done % 20 == 0) println("bucket " + done + "/" + buckets.size());
        }
        println("done: " + buckets.size() + " buckets in " + out);
    }
}
