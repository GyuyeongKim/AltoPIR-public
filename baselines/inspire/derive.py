import hashlib
import re
import sys

src, out = sys.argv[1], sys.argv[2]
text = open(src).read()

seeds = [hashlib.sha256(b"InsPIRe-paper-RGSW-public-mask-v1\x00" + i.to_bytes(4, "little")).digest() for i in range(6)]
block = "pub const RGSW_SEEDS: [[u8; 32]; 6] = [\n" + "".join("    [" + ", ".join(map(str, s)) + "],\n" for s in seeds) + "];"
masks, n = re.subn(r"pub const RGSW_SEEDS: \[\[u8; 32\]; 6\] = \[.*?\n\];", lambda _: block, text, flags=re.S)
assert n == 1
open(f"{out}/paper_masks.rs", "w").write(masks)

begin = text.index("pub fn deserialize_everything<'a>(")
end = text.index("\n\n\npub fn run_simple_ypir_rgsw_on_params(", begin)
parser = text[begin:end]
for old, new in (("    all_u8: Vec<u8>,\n)", "    all_u8: Vec<u8>,\n    public_context: &PackParams<'a>,\n)"),
                 ("    let packing_params = PackParams::new(&params, params.poly_len);", "    let packing_params = public_context.clone();")):
    assert parser.count(old) == 1
    parser = parser.replace(old, new)
head = ("use inspire::packing::{PackParams,PackingKeys,PackingType};\n"
        "use spiral_rs::aligned_memory::AlignedMemory64;\n"
        "use spiral_rs::params::Params;\n"
        "use spiral_rs::poly::{PolyMatrix,PolyMatrixNTT};\n\n")
open(f"{out}/context_parser.rs", "w").write(head + parser + "\n")
