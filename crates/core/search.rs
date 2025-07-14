/*!
Defines a very high level "search worker" abstraction.

A search worker manages the high level interaction points between the matcher
(i.e., which regex engine is used), the searcher (i.e., how data is actually
read and matched using the regex engine) and the printer. For example, the
search worker is where things like preprocessors or decompression happens.
*/

use std::{io, path::Path};

use {grep::matcher::Matcher, termcolor::WriteColor};

/// The configuration for the search worker.
///
/// Among a few other things, the configuration primarily controls the way we
/// show search results to users at a very high level.
#[derive(Clone, Debug)]
struct Config {
    preprocessor: Option<std::path::PathBuf>,
    preprocessor_globs: ignore::overrides::Override,
    search_zip: bool,
    binary_implicit: grep::searcher::BinaryDetection,
    binary_explicit: grep::searcher::BinaryDetection,
    use_ast_context: bool,
    syntax_highlighting: bool,
    semantic_search: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            preprocessor: None,
            preprocessor_globs: ignore::overrides::Override::empty(),
            search_zip: false,
            binary_implicit: grep::searcher::BinaryDetection::none(),
            binary_explicit: grep::searcher::BinaryDetection::none(),
            use_ast_context: false,
            syntax_highlighting: true, // Default to true
            semantic_search: false,
        }
    }
}

/// A builder for configuring and constructing a search worker.
#[derive(Clone, Debug)]
pub(crate) struct SearchWorkerBuilder {
    config: Config,
    command_builder: grep::cli::CommandReaderBuilder,
    decomp_builder: grep::cli::DecompressionReaderBuilder,
    pattern: Option<String>,
}

impl Default for SearchWorkerBuilder {
    fn default() -> SearchWorkerBuilder {
        SearchWorkerBuilder::new()
    }
}

impl SearchWorkerBuilder {
    /// Create a new builder for configuring and constructing a search worker.
    pub(crate) fn new() -> SearchWorkerBuilder {
        let mut cmd_builder = grep::cli::CommandReaderBuilder::new();
        cmd_builder.async_stderr(true);

        let mut decomp_builder = grep::cli::DecompressionReaderBuilder::new();
        decomp_builder.async_stderr(true);

        SearchWorkerBuilder {
            config: Config::default(),
            command_builder: cmd_builder,
            decomp_builder,
            pattern: None,
        }
    }

    /// Create a new search worker using the given searcher, matcher and
    /// printer.
    pub(crate) fn build<W: WriteColor>(
        &self,
        matcher: PatternMatcher,
        searcher: grep::searcher::Searcher,
        printer: Printer<W>,
    ) -> SearchWorker<W> {
        let config = self.config.clone();
        let command_builder = self.command_builder.clone();
        let decomp_builder = self.decomp_builder.clone();
        SearchWorker {
            config,
            command_builder,
            decomp_builder,
            matcher,
            searcher,
            printer,
            pattern: self.pattern.clone(),
        }
    }

    /// Set the path to a preprocessor command.
    ///
    /// When this is set, instead of searching files directly, the given
    /// command will be run with the file path as the first argument, and the
    /// output of that command will be searched instead.
    pub(crate) fn preprocessor(
        &mut self,
        cmd: Option<std::path::PathBuf>,
    ) -> anyhow::Result<&mut SearchWorkerBuilder> {
        if let Some(ref prog) = cmd {
            let bin = grep::cli::resolve_binary(prog)?;
            self.config.preprocessor = Some(bin);
        } else {
            self.config.preprocessor = None;
        }
        Ok(self)
    }

    /// Set the globs for determining which files should be run through the
    /// preprocessor. By default, with no globs and a preprocessor specified,
    /// every file is run through the preprocessor.
    pub(crate) fn preprocessor_globs(
        &mut self,
        globs: ignore::overrides::Override,
    ) -> &mut SearchWorkerBuilder {
        self.config.preprocessor_globs = globs;
        self
    }

    /// Enable the decompression and searching of common compressed files.
    ///
    /// When enabled, if a particular file path is recognized as a compressed
    /// file, then it is decompressed before searching.
    ///
    /// Note that if a preprocessor command is set, then it overrides this
    /// setting.
    pub(crate) fn search_zip(
        &mut self,
        yes: bool,
    ) -> &mut SearchWorkerBuilder {
        self.config.search_zip = yes;
        self
    }

    /// Set the binary detection that should be used when searching files
    /// found via a recursive directory search.
    ///
    /// Generally, this binary detection may be
    /// `grep::searcher::BinaryDetection::quit` if we want to skip binary files
    /// completely.
    ///
    /// By default, no binary detection is performed.
    pub(crate) fn binary_detection_implicit(
        &mut self,
        detection: grep::searcher::BinaryDetection,
    ) -> &mut SearchWorkerBuilder {
        self.config.binary_implicit = detection;
        self
    }

    /// Set the binary detection that should be used when searching files
    /// explicitly supplied by an end user.
    ///
    /// Generally, this binary detection should NOT be
    /// `grep::searcher::BinaryDetection::quit`, since we never want to
    /// automatically filter files supplied by the end user.
    ///
    /// By default, no binary detection is performed.
    pub(crate) fn binary_detection_explicit(
        &mut self,
        detection: grep::searcher::BinaryDetection,
    ) -> &mut SearchWorkerBuilder {
        self.config.binary_explicit = detection;
        self
    }

    /// Set whether to use AST-based enclosing symbol context.
    ///
    /// When enabled, the search worker will use AST parsing to find
    /// enclosing symbols (functions, classes, etc.) around matches
    /// instead of showing fixed line-based context.
    ///
    /// By default, AST context is disabled.
    pub(crate) fn ast_context(
        &mut self,
        yes: bool,
    ) -> &mut SearchWorkerBuilder {
        self.config.use_ast_context = yes;
        self
    }

    /// Set whether to enable syntax highlighting.
    ///
    /// By default, syntax highlighting is disabled.
    pub(crate) fn syntax_highlighting(
        &mut self,
        yes: bool,
    ) -> &mut SearchWorkerBuilder {
        self.config.syntax_highlighting = yes;
        self
    }

    /// Set whether to enable semantic search using vector embeddings.
    ///
    /// By default, semantic search is disabled.
    pub(crate) fn semantic_search(
        &mut self,
        yes: bool,
    ) -> &mut SearchWorkerBuilder {
        self.config.semantic_search = yes;
        self
    }

    /// Set the search pattern for semantic search operations.
    pub(crate) fn pattern(
        &mut self,
        pattern: Option<String>,
    ) -> &mut SearchWorkerBuilder {
        self.pattern = pattern;
        self
    }
}

/// The result of executing a search.
///
/// Generally speaking, the "result" of a search is sent to a printer, which
/// writes results to an underlying writer such as stdout or a file. However,
/// every search also has some aggregate statistics or meta data that may be
/// useful to higher level routines.
#[derive(Clone, Debug, Default)]
pub(crate) struct SearchResult {
    has_match: bool,
    stats: Option<grep::printer::Stats>,
}

impl SearchResult {
    /// Whether the search found a match or not.
    pub(crate) fn has_match(&self) -> bool {
        self.has_match
    }

    /// Return aggregate search statistics for a single search, if available.
    ///
    /// It can be expensive to compute statistics, so these are only present
    /// if explicitly enabled in the printer provided by the caller.
    pub(crate) fn stats(&self) -> Option<&grep::printer::Stats> {
        self.stats.as_ref()
    }
}

/// The pattern matcher used by a search worker.
#[derive(Clone, Debug)]
pub(crate) enum PatternMatcher {
    RustRegex(grep::regex::RegexMatcher),
    #[cfg(feature = "pcre2")]
    PCRE2(grep::pcre2::RegexMatcher),
}

/// The printer used by a search worker.
///
/// The `W` type parameter refers to the type of the underlying writer.
#[derive(Clone, Debug)]
pub(crate) enum Printer<W> {
    /// Use the standard printer, which supports the classic grep-like format.
    Standard(grep::printer::Standard<W>),
    /// Use the summary printer, which supports aggregate displays of search
    /// results.
    Summary(grep::printer::Summary<W>),
    /// A JSON printer, which emits results in the JSON Lines format.
    JSON(grep::printer::JSON<W>),
}

impl<W: WriteColor> Printer<W> {
    /// Return a mutable reference to the underlying printer's writer.
    pub(crate) fn get_mut(&mut self) -> &mut W {
        match *self {
            Printer::Standard(ref mut p) => p.get_mut(),
            Printer::Summary(ref mut p) => p.get_mut(),
            Printer::JSON(ref mut p) => p.get_mut(),
        }
    }
}

/// A worker for executing searches.
///
/// It is intended for a single worker to execute many searches, and is
/// generally intended to be used from a single thread. When searching using
/// multiple threads, it is better to create a new worker for each thread.
#[derive(Clone, Debug)]
pub(crate) struct SearchWorker<W> {
    config: Config,
    command_builder: grep::cli::CommandReaderBuilder,
    decomp_builder: grep::cli::DecompressionReaderBuilder,
    matcher: PatternMatcher,
    searcher: grep::searcher::Searcher,
    printer: Printer<W>,
    pattern: Option<String>,
}

impl<W: WriteColor> SearchWorker<W> {
    /// Execute a search over the given haystack.
    pub(crate) fn search(
        &mut self,
        haystack: &crate::haystack::Haystack,
    ) -> io::Result<SearchResult> {
        let bin = if haystack.is_explicit() {
            self.config.binary_explicit.clone()
        } else {
            self.config.binary_implicit.clone()
        };
        let path = haystack.path();
        log::trace!("{}: binary detection: {:?}", path.display(), bin);

        self.searcher.set_binary_detection(bin);
        if haystack.is_stdin() {
            self.search_reader(path, &mut io::stdin().lock())
        } else if self.should_preprocess(path) {
            self.search_preprocessor(path)
        } else if self.should_decompress(path) {
            self.search_decompress(path)
        } else {
            self.search_path(path)
        }
    }

    /// Return a mutable reference to the underlying printer.
    pub(crate) fn printer(&mut self) -> &mut Printer<W> {
        &mut self.printer
    }

    /// Returns true if and only if the given file path should be
    /// decompressed before searching.
    fn should_decompress(&self, path: &Path) -> bool {
        if !self.config.search_zip {
            return false;
        }
        self.decomp_builder.get_matcher().has_command(path)
    }

    /// Returns true if and only if the given file path should be run through
    /// the preprocessor.
    fn should_preprocess(&self, path: &Path) -> bool {
        if !self.config.preprocessor.is_some() {
            return false;
        }
        if self.config.preprocessor_globs.is_empty() {
            return true;
        }
        !self.config.preprocessor_globs.matched(path, false).is_ignore()
    }

    /// Search the given file path by first asking the preprocessor for the
    /// data to search instead of opening the path directly.
    fn search_preprocessor(
        &mut self,
        path: &Path,
    ) -> io::Result<SearchResult> {
        use std::{fs::File, process::Stdio};

        let bin = self.config.preprocessor.as_ref().unwrap();
        let mut cmd = std::process::Command::new(bin);
        cmd.arg(path).stdin(Stdio::from(File::open(path)?));

        let mut rdr = self.command_builder.build(&mut cmd).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "preprocessor command could not start: '{:?}': {}",
                    cmd, err,
                ),
            )
        })?;
        let result = self.search_reader(path, &mut rdr).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("preprocessor command failed: '{:?}': {}", cmd, err),
            )
        });
        let close_result = rdr.close();
        let search_result = result?;
        close_result?;
        Ok(search_result)
    }

    /// Attempt to decompress the data at the given file path and search the
    /// result. If the given file path isn't recognized as a compressed file,
    /// then search it without doing any decompression.
    fn search_decompress(&mut self, path: &Path) -> io::Result<SearchResult> {
        let mut rdr = self.decomp_builder.build(path)?;
        let result = self.search_reader(path, &mut rdr);
        let close_result = rdr.close();
        let search_result = result?;
        close_result?;
        Ok(search_result)
    }

    /// Search the contents of the given file path.
    fn search_path(&mut self, path: &Path) -> io::Result<SearchResult> {
        use self::PatternMatcher::*;

        let (searcher, printer) = (&mut self.searcher, &mut self.printer);
        let use_ast_context = self.config.use_ast_context;
        let syntax_highlighting = self.config.syntax_highlighting;
        let semantic_search = self.config.semantic_search;
        let pattern = self.pattern.as_deref();
        match self.matcher {
            RustRegex(ref m) => search_path_with_context(
                m,
                searcher,
                printer,
                path,
                use_ast_context,
                syntax_highlighting,
                semantic_search,
                pattern,
            ),
            #[cfg(feature = "pcre2")]
            PCRE2(ref m) => search_path_with_context(
                m,
                searcher,
                printer,
                path,
                use_ast_context,
                syntax_highlighting,
                semantic_search,
                pattern,
            ),
        }
    }

    /// Executes a search on the given reader, which may or may not correspond
    /// directly to the contents of the given file path. Instead, the reader
    /// may actually cause something else to be searched (for example, when
    /// a preprocessor is set or when decompression is enabled). In those
    /// cases, the file path is used for visual purposes only.
    ///
    /// Generally speaking, this method should only be used when there is no
    /// other choice. Searching via `search_path` provides more opportunities
    /// for optimizations (such as memory maps).
    fn search_reader<R: io::Read>(
        &mut self,
        path: &Path,
        rdr: &mut R,
    ) -> io::Result<SearchResult> {
        use self::PatternMatcher::*;

        let (searcher, printer) = (&mut self.searcher, &mut self.printer);
        match self.matcher {
            RustRegex(ref m) => search_reader(m, searcher, printer, path, rdr),
            #[cfg(feature = "pcre2")]
            PCRE2(ref m) => search_reader(m, searcher, printer, path, rdr),
        }
    }
}

/// Search the contents of the given file path using the given matcher,
/// searcher and printer, with optional AST context mode and semantic search.
fn search_path_with_context<M: Matcher, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
    use_ast_context: bool,
    syntax_highlighting: bool,
    semantic_search: bool,
    pattern: Option<&str>,
) -> io::Result<SearchResult> {
    if semantic_search {
        search_path_semantic(matcher, searcher, printer, path, pattern)
    } else if use_ast_context {
        search_path_ast_context(matcher, searcher, printer, path, syntax_highlighting)
    } else {
        search_path_standard(matcher, searcher, printer, path)
    }
}

/// Search using standard ripgrep context.
fn search_path_standard<M: Matcher, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
) -> io::Result<SearchResult> {
    match *printer {
        Printer::Standard(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_path(&matcher, path, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().map(|s| s.clone()),
            })
        }
        Printer::Summary(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_path(&matcher, path, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().map(|s| s.clone()),
            })
        }
        Printer::JSON(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_path(&matcher, path, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: Some(sink.stats().clone()),
            })
        }
    }
}

/// Search using semantic vector embeddings.
fn search_path_semantic<M: Matcher, W: WriteColor>(
    _matcher: M,
    _searcher: &mut grep::searcher::Searcher,
    _printer: &mut Printer<W>,
    path: &Path,
    pattern: Option<&str>,
) -> io::Result<SearchResult> {
    use grep::searcher::{
        SemanticConfig, SemanticSearcher,
        is_supported_file, create_ast_calculator_for_file, default_context_types,
    };
    use grep::searcher::semantic::{generate_embedding, build_index};

    // Check if this file type supports semantic search
    if !is_supported_file(path) {
        return Ok(SearchResult { has_match: false, stats: None });
    }
    
    // Read the file content
    let content = std::fs::read_to_string(path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to read file for semantic search: {}", e),
        )
    })?;

    // Create AST calculator to extract functions
    let ast_calculator = create_ast_calculator_for_file(
        path,
        &content,
        Some(default_context_types()),
    ).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("AST parsing failed: {}", e),
        )
    })?;

    // Extract individual functions using AST
    let config = SemanticConfig::default();
    let mut embeddings = Vec::new();
    
    // Extract all symbols by scanning through the file content
    let mut symbols = Vec::new();
    let mut unique_symbols = std::collections::HashSet::new();
    
    // Scan through the file content to find all symbols
    // We'll sample positions throughout the file to discover symbols
    let sample_positions: Vec<usize> = (0..content.len())
        .step_by(50) // Sample every 50 bytes
        .collect();
    
    for pos in sample_positions {
        if let Ok(context_result) = ast_calculator.calculate_context(pos..pos+1) {
            let symbol_start = context_result.range.start;
            let symbol_end = context_result.range.end;
            let symbol_key = (symbol_start, symbol_end);
            
            // Only add unique symbols (avoid duplicates)
            if unique_symbols.insert(symbol_key) {
                symbols.push(context_result);
            }
        }
    }
    
    // Extracted symbols for semantic search
    
    if symbols.is_empty() {
        // Fallback: create embedding for entire file if no symbols found
        let embedding = generate_embedding(&content, &config);
        embeddings.push((embedding, 0..content.len(), content.clone()));
    } else {
        // Create embeddings for each individual symbol
        for symbol in symbols {
            let byte_range = symbol.range.clone();
            
            // Extract symbol content from file using the range
            let symbol_content = &content[byte_range.clone()];
            let embedding = generate_embedding(symbol_content, &config);
            embeddings.push((embedding, byte_range, symbol_content.to_string()));
        }
    }

    // Build semantic index
    let index = build_index(embeddings);
    
    // Create searcher and perform search
    let mut semantic_searcher = SemanticSearcher::new(config);
    semantic_searcher.set_index(index);
    
    // Use the actual search pattern
    let query = pattern.unwrap_or("search");
    let matches = semantic_searcher.search(&query);
    
    // For now, just return whether we found semantic matches
    let has_match = !matches.is_empty();
    
    if has_match {
        for semantic_match in matches.iter() {
            println!("{}:{}-{}: {:.1}% similarity", 
                     path.display(), 
                     semantic_match.byte_range.start, 
                     semantic_match.byte_range.end,
                     semantic_match.similarity * 100.0);
            println!("{}", semantic_match.content);
        }
    }

    Ok(SearchResult { has_match, stats: None })
}

/// Search using AST-based enclosing symbol context.
fn search_path_ast_context<M: Matcher, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
    syntax_highlighting: bool,
) -> io::Result<SearchResult> {
    use grep::searcher::{
        create_ast_calculator_for_file, default_context_types,
        is_supported_file,
    };

    // Check if this file type supports AST parsing - if not, skip entirely
    if !is_supported_file(path) {
        return Ok(SearchResult { has_match: false, stats: None });
    }

    // Read the file content for AST parsing
    let content = std::fs::read_to_string(path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to read file for AST parsing: {}", e),
        )
    })?;

    // Create AST calculator
    let ast_calculator = create_ast_calculator_for_file(
        path,
        &content,
        Some(default_context_types()),
    )
    .map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("AST parsing failed: {}", e),
        )
    })?;

    // Find all matches first using a temporary sink
    let mut temp_matches = Vec::new();
    {
        let mut collector = MatchCollector::new(&mut temp_matches);
        searcher.search_path(&matcher, path, &mut collector)?;
    }

    if temp_matches.is_empty() {
        return Ok(SearchResult { has_match: false, stats: None });
    }

    // Create AST-aware sink that uses the proper printer infrastructure
    let mut ast_sink = AstSymbolSink::new(
        printer,
        &matcher,
        path,
        ast_calculator,
        content,
        temp_matches,
        syntax_highlighting,
    );

    // Process all the matches through the AST sink
    let has_match = ast_sink.process_matches(&mut *searcher)?;

    Ok(SearchResult { has_match, stats: ast_sink.stats() })
}

/// A simple sink that collects match byte ranges.
struct MatchCollector<'a> {
    matches: &'a mut Vec<(usize, usize)>,
}

impl<'a> MatchCollector<'a> {
    fn new(matches: &'a mut Vec<(usize, usize)>) -> Self {
        Self { matches }
    }
}

impl<'a> grep::searcher::Sink for MatchCollector<'a> {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        mat: &grep::searcher::SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        let start = mat.absolute_byte_offset() as usize;
        let end = start + mat.bytes().len();
        self.matches.push((start, end));
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        _context: &grep::searcher::SinkContext<'_>,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn context_break(
        &mut self,
        _searcher: &grep::searcher::Searcher,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn begin(
        &mut self,
        _searcher: &grep::searcher::Searcher,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn finish(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        _finish: &grep::searcher::SinkFinish,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Syntax highlighter that applies colors to different AST node types.
struct SyntaxHighlighter {
    colors: SyntaxColors,
}

/// Color scheme for syntax highlighting.
struct SyntaxColors {
    keyword: String,
    string: String,
    comment: String,
    number: String,
    identifier: String,
    function: String,
    type_name: String,
    operator: String,
    punctuation: String,
    normal: String,
}

impl SyntaxColors {
    fn new() -> Self {
        Self {
            keyword: "\x1b[35m".to_string(),     // Purple
            string: "\x1b[32m".to_string(),      // Green
            comment: "\x1b[90m".to_string(),     // Gray
            number: "\x1b[36m".to_string(),      // Cyan
            identifier: "\x1b[37m".to_string(),  // White
            function: "\x1b[33m".to_string(),    // Yellow
            type_name: "\x1b[34m".to_string(),   // Blue
            operator: "\x1b[91m".to_string(),    // Bright red
            punctuation: "\x1b[37m".to_string(), // White
            normal: "\x1b[0m".to_string(),       // Reset
        }
    }
}

impl SyntaxHighlighter {
    fn new() -> Self {
        Self { colors: SyntaxColors::new() }
    }

    /// Apply syntax highlighting to source code using AST information.
    fn highlight_with_ast(
        &self,
        source: &str,
        ast_calculator: &grep::searcher::AstContextCalculatorWrapper,
        symbol_offset: usize,
    ) -> String {
        match ast_calculator {
            grep::searcher::AstContextCalculatorWrapper::Calculator(calc) => {
                self.highlight_with_ast_nodes(source, calc, symbol_offset)
            }
        }
    }

    fn highlight_with_ast_nodes(
        &self,
        source: &str,
        calc: &Box<dyn grep::searcher::AstCalculator>,
        symbol_offset: usize,
    ) -> String {
        // Get AST nodes for the full file
        let syntax_nodes = calc.get_syntax_nodes();
        
        if syntax_nodes.is_empty() {
            return source.to_string();
        }
        
        // NOW WE CAN DO THIS PROPERLY!
        // Convert file-relative ranges to source-relative ranges
        let source_end = symbol_offset + source.len();
        let relevant_nodes: Vec<_> = syntax_nodes
            .into_iter()
            .filter_map(|(range, kind)| {
                // Only keep nodes that overlap with our source excerpt
                if range.end <= symbol_offset || range.start >= source_end {
                    return None; // Node is outside our excerpt
                }
                
                // Adjust range to be relative to source start
                let source_start = range.start.saturating_sub(symbol_offset);
                let source_range_end = (range.end.saturating_sub(symbol_offset)).min(source.len());
                
                if source_start >= source_range_end {
                    return None; // Invalid range
                }
                
                Some((source_start..source_range_end, kind))
            })
            .collect();
        
        if relevant_nodes.is_empty() {
            return source.to_string();
        }
        
        // Apply highlighting using the adjusted ranges
        let mut result = String::new();
        let mut current_pos = 0;
        let source_bytes = source.as_bytes();
        
        for (range, kind) in relevant_nodes {
            // Add unhighlighted text before this node
            if range.start > current_pos {
                if let Ok(text) = std::str::from_utf8(&source_bytes[current_pos..range.start]) {
                    result.push_str(text);
                }
            }
            
            // Add highlighted node
            if let Ok(node_text) = std::str::from_utf8(&source_bytes[range.start..range.end]) {
                result.push_str(&self.colorize_by_ast_kind(node_text, &kind));
            }
            
            current_pos = range.end;
        }
        
        // Add remaining unhighlighted text
        if current_pos < source.len() {
            if let Ok(text) = std::str::from_utf8(&source_bytes[current_pos..]) {
                result.push_str(text);
            }
        }
        
        result
    }
    
    fn highlight_with_smart_patterns(&self, source: &str) -> String {
        // Smarter pattern-based highlighting that avoids false positives
        let mut result = source.to_string();
        
        // Only highlight keywords in specific contexts to avoid false positives
        let rust_keywords = [
            ("fn ", "keyword"),      // Function declarations
            ("let ", "keyword"),     // Variable declarations 
            ("if ", "keyword"),      // Control flow
            ("else", "keyword"),     // Control flow
            ("for ", "keyword"),     // Loops
            ("while ", "keyword"),   // Loops
            ("match ", "keyword"),   // Pattern matching (only with space after)
            ("return", "keyword"),   // Return statements
            ("struct ", "keyword"),  // Type definitions
            ("enum ", "keyword"),    // Type definitions
            ("impl ", "keyword"),    // Implementations
            ("trait ", "keyword"),   // Trait definitions
            ("pub ", "keyword"),     // Visibility
            ("use ", "keyword"),     // Imports
            ("mod ", "keyword"),     // Modules
        ];
        
        let python_keywords = [
            ("def ", "keyword"),
            ("class ", "keyword"),
            ("if ", "keyword"),
            ("elif ", "keyword"),
            ("else:", "keyword"),
            ("for ", "keyword"),
            ("while ", "keyword"),
            ("try:", "keyword"),
            ("except", "keyword"),
            ("finally:", "keyword"),
            ("import ", "keyword"),
            ("from ", "keyword"),
            ("return", "keyword"),
        ];
        
        // Apply Rust keyword highlighting
        for (pattern, kind) in rust_keywords.iter() {
            result = self.highlight_pattern(&result, pattern, kind);
        }
        
        // Apply Python keyword highlighting  
        for (pattern, kind) in python_keywords.iter() {
            result = self.highlight_pattern(&result, pattern, kind);
        }
        
        // Highlight strings
        result = self.highlight_strings(result);
        
        // Highlight comments
        result = self.highlight_comments(result);
        
        result
    }
    
    fn highlight_pattern(&self, source: &str, pattern: &str, kind: &str) -> String {
        let mut result = String::new();
        let mut last_end = 0;
        
        for start in source.match_indices(pattern).map(|(i, _)| i) {
            // Add text before the pattern
            result.push_str(&source[last_end..start]);
            
            // Add highlighted pattern
            let end = start + pattern.len();
            result.push_str(&self.colorize_by_ast_kind(&source[start..end], kind));
            
            last_end = end;
        }
        
        // Add remaining text
        result.push_str(&source[last_end..]);
        result
    }
    
    fn highlight_strings(&self, source: String) -> String {
        let mut result = source;
        
        // Handle double-quoted strings
        let mut new_result = String::new();
        let mut chars = result.chars().peekable();
        let mut in_string = false;
        let mut string_start = 0;
        let mut current_string = String::new();
        let mut pos = 0;
        
        while let Some(ch) = chars.next() {
            if ch == '"' && !in_string {
                // Start of string
                new_result.push_str(&result[string_start..pos]);
                in_string = true;
                current_string.clear();
                current_string.push(ch);
                string_start = pos;
            } else if ch == '"' && in_string {
                // End of string
                current_string.push(ch);
                new_result.push_str(&self.colorize_by_ast_kind(&current_string, "string"));
                in_string = false;
                string_start = pos + 1;
            } else if in_string {
                current_string.push(ch);
            }
            pos += ch.len_utf8();
        }
        
        // Add any remaining text
        if string_start < result.len() {
            new_result.push_str(&result[string_start..]);
        }
        
        new_result
    }
    
    fn highlight_comments(&self, source: String) -> String {
        let mut result = String::new();
        
        for line in source.lines() {
            if let Some(comment_start) = line.find("//") {
                // Add text before comment
                result.push_str(&line[..comment_start]);
                // Add highlighted comment
                result.push_str(&self.colorize_by_ast_kind(&line[comment_start..], "comment"));
            } else if let Some(comment_start) = line.find("#") {
                // Python-style comment
                result.push_str(&line[..comment_start]);
                result.push_str(&self.colorize_by_ast_kind(&line[comment_start..], "comment"));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
        
        // Remove trailing newline if source didn't have one
        if !source.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }
        
        result
    }

    fn colorize_by_ast_kind(&self, text: &str, kind: &str) -> String {
        let color = match kind {
            // Rust/JavaScript/Python/Go keywords - using AST semantic types
            kind if kind.contains("keyword")
                || kind == "fn"
                || kind == "let"
                || kind == "const"
                || kind == "function"
                || kind == "def"
                || kind == "class"
                || kind == "if"
                || kind == "else"
                || kind == "for"
                || kind == "while"
                || kind == "return"
                || kind == "import"
                || kind == "export"
                || kind == "struct"
                || kind == "enum"
                || kind == "impl"
                || kind == "trait"
                || kind == "pub"
                || kind == "async"
                || kind == "await" =>
            {
                &self.colors.keyword
            }

            // String literals
            kind if kind.contains("string")
                || kind.contains("char_literal") =>
            {
                &self.colors.string
            }

            // Numbers
            kind if kind.contains("number")
                || kind.contains("integer")
                || kind.contains("float")
                || kind.contains("decimal") =>
            {
                &self.colors.number
            }

            // Comments
            kind if kind.contains("comment") => &self.colors.comment,

            // Function names and calls
            kind if kind.contains("function")
                || kind.contains("call")
                || kind == "function_item"
                || kind == "function_declaration" =>
            {
                &self.colors.function
            }

            // Type identifiers
            kind if kind.contains("type")
                || kind == "type_identifier"
                || kind.contains("primitive_type") =>
            {
                &self.colors.type_name
            }

            // Operators
            kind if kind.contains("operator")
                || kind.contains("binary")
                || kind.contains("unary")
                || kind.contains("assignment") =>
            {
                &self.colors.operator
            }

            _ => &self.colors.normal,
        };

        format!("{}{}{}", color, text, self.colors.normal)
    }
}

/// AST-aware sink that outputs enclosing symbols with proper formatting.
struct AstSymbolSink<'a, M, W> {
    printer: &'a mut Printer<W>,
    matcher: &'a M,
    path: &'a Path,
    ast_calculator: grep::searcher::AstContextCalculatorWrapper,
    content: String,
    original_matches: Vec<(usize, usize)>,
    has_match: bool,
    syntax_highlighting: bool,
}

impl<'a, M: Matcher, W: WriteColor> AstSymbolSink<'a, M, W> {
    fn new(
        printer: &'a mut Printer<W>,
        matcher: &'a M,
        path: &'a Path,
        ast_calculator: grep::searcher::AstContextCalculatorWrapper,
        content: String,
        original_matches: Vec<(usize, usize)>,
        syntax_highlighting: bool,
    ) -> Self {
        Self {
            printer,
            matcher,
            path,
            ast_calculator,
            content,
            original_matches,
            has_match: false,
            syntax_highlighting,
        }
    }

    fn process_matches(
        &mut self,
        searcher: &mut grep::searcher::Searcher,
    ) -> io::Result<bool> {
        let mut output_ranges = std::collections::HashSet::new();
        let matches_copy = self.original_matches.clone();

        for (match_start, match_end) in matches_copy {
            let match_range = match_start..match_end;

            match self.ast_calculator.calculate_context(match_range) {
                Ok(context_result) => {
                    // Avoid outputting the same symbol multiple times
                    if output_ranges.insert((
                        context_result.range.start,
                        context_result.range.end,
                    )) {
                        self.output_symbol(searcher, &context_result)?;
                        self.has_match = true;
                    }
                }
                Err(_ast_error) => {
                    // Skip matches that don't have enclosing symbols
                }
            }
        }

        Ok(self.has_match)
    }

    fn output_symbol(
        &mut self,
        _searcher: &mut grep::searcher::Searcher,
        context_result: &grep::searcher::AstContextResult,
    ) -> io::Result<()> {
        let symbol_start = context_result.range.start;
        let symbol_end = context_result.range.end;

        // Print file path header
        println!("\x1b[36m{}\x1b[0m", self.path.display()); // Cyan file path

        // Extract the symbol content
        let symbol_content = &self.content[symbol_start..symbol_end];

        // Apply AST-based syntax highlighting if enabled
        let highlighted_content = if self.syntax_highlighting {
            let highlighter = SyntaxHighlighter::new();
            highlighter.highlight_with_ast(symbol_content, &self.ast_calculator, symbol_start)
        } else {
            symbol_content.to_string()
        };

        // Add line numbers to the output with match highlighting
        let start_line = self.byte_to_line(symbol_start);
        let original_lines: Vec<&str> = symbol_content.lines().collect();

        for (i, line) in highlighted_content.lines().enumerate() {
            let current_line = start_line + i;
            let original_line = original_lines.get(i).unwrap_or(&"");

            // Calculate byte positions for this line within the symbol
            let line_start_byte = symbol_start
                + original_lines
                    .iter()
                    .take(i)
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum::<usize>();
            let line_end_byte = line_start_byte + original_line.len();

            // Find matches within this line
            let line_matches: Vec<(usize, usize)> = self.original_matches
                .iter()
                .filter_map(|(match_start, match_end)| {
                    if *match_start >= line_start_byte && *match_start < line_end_byte {
                        // Convert to line-relative positions
                        let line_match_start = match_start.saturating_sub(line_start_byte);
                        let line_match_end = (*match_end).min(line_end_byte).saturating_sub(line_start_byte);
                        Some((line_match_start, line_match_end))
                    } else {
                        None
                    }
                })
                .collect();

            let final_line = if !line_matches.is_empty() {
                // For lines with matches, apply highlighting to original line first, then syntax
                let match_highlighted = self.highlight_search_matches_simple(original_line, &line_matches);
                if self.syntax_highlighting {
                    // Apply syntax highlighting while preserving search match highlighting
                    self.apply_syntax_around_matches(&match_highlighted, &line_matches)
                } else {
                    match_highlighted
                }
            } else {
                line.to_string()
            };

            if !line_matches.is_empty() {
                println!("\x1b[1;32m{}\x1b[0m:{}", current_line, final_line); // Green bold line number
            } else {
                println!("{}:{}", current_line, final_line);
            }
        }

        Ok(())
    }

    fn byte_to_line(&self, byte_offset: usize) -> usize {
        self.content.bytes().take(byte_offset).filter(|&b| b == b'\n').count()
            + 1
    }

    fn stats(&self) -> Option<grep::printer::Stats> {
        // For now, return None - we could implement proper stats later
        None
    }
    
    fn highlight_search_matches_simple(&self, line: &str, matches: &[(usize, usize)]) -> String {
        if matches.is_empty() {
            return line.to_string();
        }
        
        // Debug: check if all matches are out of bounds
        let valid_matches: Vec<_> = matches.iter()
            .filter(|(start, end)| *start < line.len() && *end <= line.len() && start < end)
            .collect();
            
        if valid_matches.is_empty() {
            // No valid matches within this line - highlight entire line for now to show something is matching
            // TODO: Fix the position calculation
            return format!("\x1b[1;48;2;212;147;113m{}\x1b[0m", line);
        }
        
        let mut result = String::new();
        let mut last_pos = 0;
        
        for (start, end) in valid_matches {
            // Add text before match
            if *start > last_pos {
                result.push_str(&line[last_pos..*start]);
            }
            
            // Add highlighted match - bright red background  
            result.push_str("\x1b[1;48;2;212;147;113m"); // Custom RGB background
            result.push_str(&line[*start..*end]);
            result.push_str("\x1b[0m"); // Reset
            
            last_pos = *end;
        }
        
        // Add remaining text
        if last_pos < line.len() {
            result.push_str(&line[last_pos..]);
        }
        
        result
    }
    
    fn apply_syntax_around_matches(&self, line: &str, _matches: &[(usize, usize)]) -> String {
        // For now, let's keep it simple - just return the line with match highlighting
        // The search highlighting takes precedence
        line.to_string()
    }
}

/// Legacy function for compatibility.
fn search_path<M: Matcher, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
) -> io::Result<SearchResult> {
    search_path_standard(matcher, searcher, printer, path)
}

/// Search the contents of the given reader using the given matcher, searcher
/// and printer.
fn search_reader<M: Matcher, R: io::Read, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
    mut rdr: R,
) -> io::Result<SearchResult> {
    match *printer {
        Printer::Standard(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_reader(&matcher, &mut rdr, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().map(|s| s.clone()),
            })
        }
        Printer::Summary(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_reader(&matcher, &mut rdr, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().map(|s| s.clone()),
            })
        }
        Printer::JSON(ref mut p) => {
            let mut sink = p.sink_with_path(&matcher, path);
            searcher.search_reader(&matcher, &mut rdr, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: Some(sink.stats().clone()),
            })
        }
    }
}
