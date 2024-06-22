//A script for exporting the complete CFG to a file
//@author Michael Chesser
//@category custom
//@keybinding
//@menupath
//@toolbar

import ghidra.app.script.GhidraScript;

import ghidra.program.model.block.*;

import java.io.BufferedWriter;
import java.io.FileWriter;
import java.util.HashMap;

public class ExportCfg extends GhidraScript {
    @Override
    public void run() throws Exception {
        if (this.currentProgram == null) {
            return;
        }

        var outFile = this.askFile("File to export the CFG to", "Export");
        try (var writer = new BufferedWriter(new FileWriter(outFile))) {
            writer.write("{\n");

            var listing = this.currentProgram.getListing();

            // Get all functions
            writer.write("\"functions\": {\n");
            var first = true;
            for (var function : listing.getFunctions(true)) {
                var name = function.getName(true);
                // Ensure that the name has no `"` characters in it.
                name = name.replace("\"", "\\\"");

                var address = function.getEntryPoint().getOffset();

                if (first) {
                    first = false;
                } else {
                    writer.write(",\n");
                }
                writer.write(String.format("\"%d\": \"%s\"", address, name));
            }
            writer.write("\n},\n"); // End of functions

            // Keep track of all the edges we've seen, we allow one edge per (from, to) pair
            // whichmay have one of serveral types.
            var edges = new HashMap<Edge, String>();

            // Iterate over all blocks and write the range of addresses to the file
            writer.write("\"blocks\": [\n");
            var model = new SimpleBlockModel(this.currentProgram, false);
            var iter = model.getCodeBlocks(this.monitor);
            while (iter.hasNext()) {
                var b = iter.next();
                var start = b.getMinAddress().getOffset();
                var end = b.getMaxAddress().getOffset();

                Long funcAddr = null;
                var func = listing.getFunctionContaining(b.getMinAddress());
                if (func != null) {
                    funcAddr = func.getEntryPoint().getOffset();
                }

                writer.write(String.format("{\"start\": %d, \"end\": %d, \"func\": %d}", start, end, funcAddr));
                // Iterate over all outgoing edges in the block
                var dests = b.getDestinations(monitor);
                while (dests.hasNext()) {
                    var blockRef = dests.next();
                    var type = blockRef.getFlowType().toString();

                    var from = blockRef.getSourceAddress().getOffset();
                    var to = blockRef.getDestinationAddress().getOffset();

                    edges.put(new Edge(from, to), type);
                }

                if (iter.hasNext()) {
                    writer.write(",\n");
                }
            }
            writer.write("\n]\n"); // End of block list

            // Iterate over all edges and write them to the file
            writer.write(",\n\"edges\": [\n");
            var firstEdge = true;
            for (var edge : edges.keySet()) {
                if (firstEdge) {
                    firstEdge = false;
                } else {
                    writer.write(",\n");
                }
                writer.write(String.format("{\"from\": %d, \"to\": %d, \"kind\": \"%s\"}", edge.from,
                        edge.to, edges.get(edge)));
            }

            writer.write("\n]\n"); // End of edge list

            writer.write("}"); // End of JSON
        }

    }

    static class Edge {
        public final long from;
        public final long to;

        public Edge(long from, long to) {
            this.from = from;
            this.to = to;
        }

        @Override
        public int hashCode() {
            final int prime = 31;
            int result = 1;
            result = prime * result + (int) (from ^ (from >>> 32));
            result = prime * result + (int) (to ^ (to >>> 32));
            return result;
        }

        @Override
        public boolean equals(Object obj) {
            if (this == obj)
                return true;
            if (obj == null)
                return false;
            if (getClass() != obj.getClass())
                return false;
            Edge other = (Edge) obj;
            if (from != other.from)
                return false;
            if (to != other.to)
                return false;
            return true;
        }

    }
}
