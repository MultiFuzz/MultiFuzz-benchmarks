use std::{
    collections::{HashMap, BTreeMap},
    path::{Path, PathBuf},
};

use anyhow::Context;

#[derive(serde::Deserialize, Clone)]
pub struct MetadataSource {
    #[serde(default)]
    block_maps: HashMap<String, PathBuf>,
}

#[derive(Default, Clone)]
pub struct Metadata {
    /// Information about the blocks for a particular binary.
    pub block_maps: Vec<BlockMap>,

    /// A mapping from the name of a binary to an index into `block_maps`
    pub binary_mapping: HashMap<String, usize>,
}

impl Metadata {
    /// Loads a config file from the given path.
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config_path = path.as_ref();
        let config: MetadataSource = {
            let file = std::fs::read(config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?;
            ron::de::from_bytes(&file)
                .with_context(|| format!("failed to parse {}", config_path.display()))?
        };
        let metadata_root = config_path.parent().unwrap_or(&Path::new("."));
        Self::from_source(metadata_root, config)
    }

    pub fn from_source(metadata_root: &Path, config: MetadataSource) -> anyhow::Result<Self> {
        let mut mapping: HashMap<PathBuf, usize> = HashMap::new();
        let mut block_maps = vec![];
        let binary_mapping = config
            .block_maps
            .into_iter()
            .map(|(key, path)| {
                use std::collections::hash_map::Entry;
                let block_map_id = match mapping.entry(path) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        let id = block_maps.len();
                        block_maps.push(parse_block_map(metadata_root, entry.key())?);
                        entry.insert(id).clone()
                    }
                };
                Ok((key, block_map_id))
            })
            .collect::<anyhow::Result<_>>()
            .with_context(|| format!("error loading block map from {}", metadata_root.display()))?;

        Ok(Self { block_maps, binary_mapping })
    }

    /// Return the block map associated with `binary`.
    pub fn get_block_map_for(&self, binary: &str) -> Option<&BlockMap> {
        let idx = self.binary_mapping.get(binary)?;
        Some(&self.block_maps[*idx])
    }
}

fn parse_block_map(config_path: &Path, path: &PathBuf) -> anyhow::Result<BlockMap> {
    let block_map_path = try_find_file(&config_path, path)
        .ok_or_else(|| anyhow::format_err!("failed to find: {}", path.display()))?;
    BlockMap::parse_from_path(&block_map_path)
        .with_context(|| format!("failed to parse {}", block_map_path.display()))
}

/// Check for a file at `path` either relative to the current working directory, or to
/// `relative_to`.
fn try_find_file(relative_to: &Path, path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.into());
    }

    let path = relative_to.join(path);
    if path.exists() {
        return Some(path);
    }

    None
}

/// Information about the structure of a program.
#[derive(Default, Clone)]
pub struct BlockMap {
    /// Tree containing a mapping from block starting address to block ranges.
    // note: blocks are guaranteed to be non-overlapping, so we can use a simple btree interval
    // mapping for efficient lookup instead of a more complicated data structure.
    interval_tree: BTreeMap<u64, Block>,

    /// Map of all known functions in the binary indexed by address.
    functions: BTreeMap<u64, Function>,

    /// The edges in the control flow graph, indexed by (source address, destination address).
    edges: BTreeMap<(u64, u64), EdgeKind>,
}

impl BlockMap {
    /// Parse a [BlockMap] from a file.
    pub fn parse_from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        match path.extension().map_or(false, |ext| ext == "txt") {
            true => Self::from_txt(path),
            false => {
                let bytes = std::fs::read(path)
                    .with_context(|| format!("Failed to read: {}", path.display()))?;
                Self::from_json(&bytes)
            }
        }
    }

    /// Get block map from a simple text file containing `start end [fallthrough]` lines.
    pub fn from_txt(path: &std::path::Path) -> anyhow::Result<Self> {
        use std::io::BufRead;

        let mut map = BTreeMap::new();
        let reader = std::io::BufReader::new(std::fs::File::open(path)?);
        for line in reader.lines() {
            let line = line?;

            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let entry = Block::parse_from_line(line)?;
            if map.insert(entry.start, entry).is_some() {
                eprintln!(
                    "[WARN] {} contains a duplicate block start address: {:#x}",
                    path.display(),
                    entry.start
                );
            }
        }

        Ok(Self { interval_tree: map, functions: BTreeMap::new(), edges: BTreeMap::new() })
    }

    /// Get block map from a JSON file containing.
    pub fn from_json(bytes: &[u8]) -> anyhow::Result<Self> {
        #[derive(serde::Deserialize)]
        struct BlockMapJson {
            functions: BTreeMap<u64, String>,
            blocks: Vec<Block>,
            #[serde(default)]
            edges: Vec<Edge>,
        }
        let data: BlockMapJson = serde_json::from_slice(bytes)?;

        let mut functions: BTreeMap<_, _> = data
            .functions
            .into_iter()
            .map(|(addr, name)| (addr, Function::with_name(addr, name)))
            .collect();

        let edges: BTreeMap<_, _> =
            data.edges.iter().map(|edge| ((edge.from, edge.to), edge.kind)).collect();

        // Create an index for outgoing edges so we can compute fallthroughs.
        let mut outgoing_edges: HashMap<_, Vec<_>> = HashMap::new();
        for edge in &data.edges {
            outgoing_edges.entry(edge.from).or_default().push((edge.to, edge.kind));
        }

        let mut interval_tree = BTreeMap::new();
        for mut entry in data.blocks {
            // Update the fall through address of blocks that always take the fallthrough edge.
            //
            // Note: we are only guaranteed to fall through to the next block if there is only one
            // outgoing edge.
            if let Some(&[(to, EdgeKind::FallThrough | EdgeKind::UnconditionalJump)]) =
                outgoing_edges.get(&entry.start).map(|x| x.as_slice())
            {
                entry.fallthrough = Some(to);
            }

            if interval_tree.insert(entry.start, entry).is_some() {
                eprintln!(
                    "[WARN] a block in program has a duplicate start address: {:#0x}",
                    entry.start
                );
            }
            if let Some(function) = entry.function {
                functions.get_mut(&function).unwrap().blocks.push(entry.start);
            }
        }

        Ok(Self { interval_tree, functions, edges })
    }

    /// Returns whether `addr` corresponds to the start of a valid block.
    pub fn is_valid_block(&self, addr: u64) -> bool {
        let Some(containing_block) = self.get_containing_block(addr) else { return false; };
        containing_block.start == addr
    }

    /// Returns the total number of blocks in the binary.
    pub fn block_count(&self) -> usize {
        self.interval_tree.len()
    }

    /// Returns an iterator over all blocks in the binary.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.interval_tree.values()
    }

    /// Returns an iterator over all edges in the binary.
    pub fn edges(&self) -> impl Iterator<Item = Edge> + '_ {
        self.edges.iter().map(|(&(from, to), &kind)| Edge { from, to, kind })
    }

    /// Get the block that contains `addr`
    pub fn get_containing_block(&self, addr: u64) -> Option<Block> {
        // Block must start before (or at) this address.
        let (_, entry) = self.interval_tree.range(..=addr).rev().next()?;

        // Block must not end before the address
        if entry.end < addr {
            return None;
        }

        Some(*entry)
    }

    /// Get the function that contains `addr`
    pub fn get_containing_function(&self, addr: u64) -> Option<&Function> {
        self.get_containing_block(addr)
            .and_then(|entry| entry.function)
            .and_then(|addr| self.functions.get(&addr))
    }

    /// Returns an iterator over all of the blocks that are always hit starting at `addr`.
    pub fn get_reachable_blocks(&self, addr: u64) -> impl Iterator<Item = Block> + '_ {
        let mut next = Some(addr);
        std::iter::from_fn(move || {
            let addr = next.take()?;
            let entry = self.get_containing_block(addr)?;
            next = entry.fallthrough;
            Some(entry)
        })
    }

    /// Get the function with entrypoint `addr`.
    pub fn get_function(&self, addr: u64) -> Option<&Function> {
        self.functions.get(&addr)
    }

    /// Find the function with the name `name`.
    pub fn find_function_by_name(&self, name: &str) -> Option<&Function> {
        self.functions.values().find(|f| f.name == name)
    }

    /// Generates a human readable name for a block, based on it's offset from the containing
    /// function.
    pub fn get_block_name(&self, addr: u64) -> String {
        let nice_name = (|| {
            let block = self.get_containing_block(addr)?;
            let function = self.get_containing_function(addr)?;
            match block.start - function.addr {
                0 => Some(format!("{}", function.name)),
                offset => Some(format!("{}+{:#0x}", function.name, offset)),
            }
        })();

        nice_name.unwrap_or_else(|| format!("{:#0x}", addr))
    }

    /// Relocate all addresses by a fixed offset
    pub fn relocate(self, offset: u64) -> Self {
        let interval_tree = self
            .interval_tree
            .into_iter()
            .map(|(start, block)| (start + offset, block.offset(offset)))
            .collect();

        let functions = self
            .functions
            .into_iter()
            .map(|(start, func)| (start + offset, func.offset(offset)))
            .collect();

        let edges = self
            .edges
            .into_iter()
            .map(|((start, end), kind)| ((start + offset, end + offset), kind))
            .collect();

        Self { interval_tree, functions, edges }
    }
}

#[derive(Clone)]
pub struct Function {
    /// The name of the function.
    pub name: String,

    /// The starting address of the function.
    pub addr: u64,

    /// The starting addresses of each block in the function.
    pub blocks: Vec<u64>,
}

impl Function {
    pub fn with_name(addr: u64, name: impl Into<String>) -> Self {
        Self { name: name.into(), addr, blocks: Vec::new() }
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.blocks.iter_mut().for_each(|x| *x += offset);
        Self { name: self.name, addr: self.addr + offset, blocks: self.blocks }
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Block {
    /// The starting address of the first instruction in the block.
    pub start: u64,

    /// The address of the last byte in the block.
    pub end: u64,

    /// The address of the instruction that will be executed after this block if the block exits
    /// unconditionally.
    #[serde(rename = "next")]
    pub fallthrough: Option<u64>,

    /// Address of the function containing this block.
    #[serde(rename = "func")]
    pub function: Option<u64>,
}

impl Block {
    /// Parse a [Block] from a whitespace delimited line: `start end [fallthrough]`
    fn parse_from_line(line: &str) -> anyhow::Result<Self> {
        let mut iter = line.split_ascii_whitespace();

        let start = match iter.next() {
            Some(x) => u64::from_str_radix(x, 16).context("error parsing `start`")?,
            None => anyhow::bail!("expected `start`"),
        };

        let end = match iter.next() {
            Some(x) => u64::from_str_radix(x, 16).context("error parsing `end`")?,
            None => anyhow::bail!("expected `end`"),
        };

        let fallthrough = match iter.next() {
            Some(x) => Some(u64::from_str_radix(x, 16).context("error parsing `fallthrough`")?),
            _ => None,
        };

        Ok(Self { start, end, fallthrough, function: None })
    }

    /// Move block by a fixed offset
    pub fn offset(self, offset: u64) -> Self {
        Self {
            start: self.start + offset,
            end: self.end + offset,
            fallthrough: self.fallthrough.map(|x| x + offset),
            function: self.function.map(|x| x + offset),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: u64,
    pub to: u64,
    pub kind: EdgeKind,
}

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    #[serde(rename = "COMPUTED_CALL")]
    ComputedCall,
    #[serde(rename = "COMPUTED_CALL_TERMINATOR")]
    ComputedCallTerminator,
    #[serde(rename = "COMPUTED_JUMP")]
    ComputedJump,
    #[serde(rename = "CONDITIONAL_JUMP")]
    ConditionalJump,
    #[serde(rename = "FALL_THROUGH")]
    FallThrough,
    #[serde(rename = "INDIRECTION")]
    Indirection,
    #[serde(rename = "UNCONDITIONAL_CALL")]
    UnconditionalCall,
    #[serde(rename = "UNCONDITIONAL_JUMP")]
    UnconditionalJump,
    #[serde(rename = "CONDITIONAL_CALL")]
    ConditionalCall,
}

impl EdgeKind {
    pub fn is_call(&self) -> bool {
        match self {
            EdgeKind::ComputedCall
            | EdgeKind::ComputedCallTerminator
            | EdgeKind::UnconditionalCall => true,
            _ => false,
        }
    }

    pub fn is_conditional(&self) -> bool {
        match self {
            EdgeKind::ConditionalJump | EdgeKind::ConditionalCall | EdgeKind::ComputedJump => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeKind::ComputedCall => write!(f, "COMPUTED_CALL"),
            EdgeKind::ComputedCallTerminator => write!(f, "COMPUTED_CALL_TERMINATOR"),
            EdgeKind::ComputedJump => write!(f, "COMPUTED_JUMP"),
            EdgeKind::ConditionalJump => write!(f, "CONDITIONAL_JUMP"),
            EdgeKind::FallThrough => write!(f, "FALL_THROUGH"),
            EdgeKind::Indirection => write!(f, "INDIRECTION"),
            EdgeKind::UnconditionalCall => write!(f, "UNCONDITIONAL_CALL"),
            EdgeKind::ConditionalCall => write!(f, "CONDITIONAL_CALL"),
            EdgeKind::UnconditionalJump => write!(f, "UNCONDITIONAL_JUMP"),
        }
    }
}
