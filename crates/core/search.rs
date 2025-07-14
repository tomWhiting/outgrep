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
        }
    }
}

/// A builder for configuring and constructing a search worker.
#[derive(Clone, Debug)]
pub(crate) struct SearchWorkerBuilder {
    config: Config,
    command_builder: grep::cli::CommandReaderBuilder,
    decomp_builder: grep::cli::DecompressionReaderBuilder,
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
        match self.matcher {
            RustRegex(ref m) => search_path_with_context(
                m,
                searcher,
                printer,
                path,
                use_ast_context,
                syntax_highlighting,
            ),
            #[cfg(feature = "pcre2")]
            PCRE2(ref m) => search_path_with_context(
                m,
                searcher,
                printer,
                path,
                use_ast_context,
                syntax_highlighting,
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
/// searcher and printer, with optional AST context mode.
fn search_path_with_context<M: Matcher, W: WriteColor>(
    matcher: M,
    searcher: &mut grep::searcher::Searcher,
    printer: &mut Printer<W>,
    path: &Path,
    use_ast_context: bool,
    syntax_highlighting: bool,
) -> io::Result<SearchResult> {
    if use_ast_context {
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
    ) -> String {
        match ast_calculator {
            grep::searcher::AstContextCalculatorWrapper::Calculator(calc) => {
                self.highlight_with_ast_nodes(source, calc)
            }
        }
    }

    fn highlight_with_ast_nodes(
        &self,
        source: &str,
        calc: &Box<dyn grep::searcher::AstCalculator>,
    ) -> String {
        // For now, let's just do a simple keyword-based highlighting
        // without using the AST ranges since they're based on the full file
        // but we're only highlighting a symbol excerpt
        
        let mut result = source.to_string();
        
        // Simple keyword highlighting
        let keywords = [
            "fn", "let", "mut", "const", "if", "else", "for", "while", "loop", "match", 
            "return", "struct", "enum", "impl", "trait", "pub", "use", "mod", 
            "def", "class", "import", "from", "elif", "try", "except", "finally",
        ];
        
        for keyword in keywords.iter() {
            // Use a simple word boundary approach
            let pattern = format!("\\b{}\\b", keyword);
            
            // Simple replace approach - find whole words and highlight them
            let mut new_result = String::new();
            let mut last_end = 0;
            
            for (start, part) in result.match_indices(keyword) {
                // Check word boundaries manually
                let before_ok = start == 0 || 
                    !result.chars().nth(start - 1).unwrap_or(' ').is_alphanumeric();
                let end = start + keyword.len();
                let after_ok = end >= result.len() || 
                    !result.chars().nth(end).unwrap_or(' ').is_alphanumeric();
                
                if before_ok && after_ok {
                    // Add text before the keyword
                    new_result.push_str(&result[last_end..start]);
                    // Add highlighted keyword
                    new_result.push_str(&self.colorize_by_ast_kind(part, "keyword"));
                    last_end = end;
                }
            }
            
            // Add remaining text
            new_result.push_str(&result[last_end..]);
            result = new_result;
        }
        
        // Highlight strings
        result = self.highlight_strings(result);
        
        // Highlight comments
        result = self.highlight_comments(result);
        
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
            highlighter.highlight_with_ast(symbol_content, &self.ast_calculator)
        } else {
            symbol_content.to_string()
        };

        // Add line numbers to the output with match highlighting
        let start_line = self.byte_to_line(symbol_start);
        let original_lines: Vec<&str> = symbol_content.lines().collect();

        for (i, line) in highlighted_content.lines().enumerate() {
            let current_line = start_line + i;
            let original_line = original_lines.get(i).unwrap_or(&"");

            // Check if this line contains any of our original matches
            let has_match = self.original_matches.iter().any(
                |(match_start, _match_end)| {
                    let line_start_byte = symbol_start
                        + original_lines
                            .iter()
                            .take(i)
                            .map(|l| l.len() + 1)
                            .sum::<usize>();
                    let line_end_byte = line_start_byte + original_line.len();
                    *match_start >= line_start_byte
                        && *match_start < line_end_byte
                },
            );

            if has_match {
                println!("\x1b[1;32m{}\x1b[0m:{}", current_line, line); // Green bold line number
            } else {
                println!("{}:{}", current_line, line);
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
