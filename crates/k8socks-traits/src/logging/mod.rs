pub trait LoggingService {
    fn init_logging(
        level_str: &str,
        use_color: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}