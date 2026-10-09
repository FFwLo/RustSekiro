// Decompiles functions for offline reading.
// Args: <out.c> then any mix of:
//   ref:<hexaddr>   every function that references this address (e.g. a string)
//   fn:<hexaddr>    the function containing this address
//   callees:<hexaddr>  the function and everything it calls directly
//   callers:<hexaddr>  every function that calls this function
//   range:<hexstart>-<hexend>  every function starting in [start, end)
//   orq:<hexoff>    list (notes only) functions with an OR qword [reg+off] instruction
//   asm:<hexaddr>   disassembly of the function containing this address (notes only)
// @category ShinobiCombat
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileRefs extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        Set<Function> fns = new LinkedHashSet<>();
        StringBuilder notes = new StringBuilder();
        for (int i = 1; i < args.length; i++) {
            String[] kv = args[i].split(":", 2);
            if (kv[0].equals("range")) {
                String[] se = kv[1].split("-");
                Address s0 = toAddr(Long.parseUnsignedLong(se[0], 16));
                Address s1 = toAddr(Long.parseUnsignedLong(se[1], 16));
                for (Function f : currentProgram.getFunctionManager().getFunctions(s0, true)) {
                    if (f.getEntryPoint().compareTo(s1) >= 0) break;
                    fns.add(f);
                }
                continue;
            }
            if (kv[0].equals("asm")) {
                Function f = getFunctionContaining(toAddr(Long.parseUnsignedLong(kv[1], 16)));
                if (f != null) {
                    notes.append("// asm ").append(f.getName()).append("\n");
                    for (ghidra.program.model.listing.Instruction ins : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                        notes.append("//   ").append(ins.getAddress()).append("  ").append(ins.toString()).append("\n");
                    }
                }
                continue;
            }
            if (kv[0].equals("orq")) {
                String needle = "+ 0x" + kv[1].toLowerCase() + "]";
                Set<String> seen = new LinkedHashSet<>();
                for (ghidra.program.model.listing.Instruction ins : currentProgram.getListing().getInstructions(true)) {
                    if (!ins.getMnemonicString().equals("OR")) continue;
                    String op = ins.getDefaultOperandRepresentation(0);
                    if (!op.startsWith("qword ptr") || !op.toLowerCase().contains(needle)) continue;
                    Function f = getFunctionContaining(ins.getAddress());
                    String n = f == null ? "?" : f.getName();
                    if (seen.add(n)) notes.append("// orq ").append(kv[1]).append(" at ").append(ins.getAddress()).append(" in ").append(n).append("\n");
                }
                continue;
            }
            Address a = toAddr(Long.parseUnsignedLong(kv[1].replace("0x", ""), 16));
            switch (kv[0]) {
                case "ref":
                    for (Reference r : getReferencesTo(a)) {
                        Function f = getFunctionContaining(r.getFromAddress());
                        notes.append("// ref ").append(a).append(" from ").append(r.getFromAddress())
                             .append(" in ").append(f == null ? "?" : f.getName()).append("\n");
                        if (f != null) fns.add(f);
                    }
                    break;
                case "fn": {
                    Function f = getFunctionContaining(a);
                    if (f != null) fns.add(f);
                    break;
                }
                case "callers": {
                    Function f = getFunctionContaining(a);
                    if (f != null) {
                        for (Function c : f.getCallingFunctions(monitor)) {
                            notes.append("// caller of ").append(f.getName()).append(": ").append(c.getName()).append(" @ ").append(c.getEntryPoint()).append("\n");
                            fns.add(c);
                        }
                    }
                    break;
                }
                case "callees": {
                    Function f = getFunctionContaining(a);
                    if (f != null) {
                        fns.add(f);
                        fns.addAll(f.getCalledFunctions(monitor));
                    }
                    break;
                }
            }
        }
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        try (PrintWriter w = new PrintWriter(args[0], "UTF-8")) {
            w.print(notes);
            for (Function f : fns) {
                DecompileResults r = d.decompileFunction(f, 120, monitor);
                w.println("\n// ===== " + f.getName() + " @ " + f.getEntryPoint());
                w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage());
            }
        }
        println("wrote " + fns.size() + " functions to " + args[0]);
    }
}
