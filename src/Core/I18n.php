<?php

namespace SecretSanta\Core;

class I18n
{
    private static ?self $instance = null;
    private string $locale;
    private array $translations = [];
    private array $loadedDomains = [];
    
    private function __construct()
    {
        // Set the default locale from session or environment
        $session = Session::getInstance();
        $this->locale = $session->get('locale', getenv('APP_LOCALE') ?: 'en');
        
        // Set locale for PHP functions
        setlocale(LC_ALL, $this->locale . '_' . strtoupper($this->locale) . '.UTF-8');
        
        // Initialize ICU
        \Locale::setDefault($this->locale);
    }
    
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    public function setLocale(string $locale): void
    {
        $this->locale = $locale;
        
        // Update PHP locale
        setlocale(LC_ALL, $locale . '_' . strtoupper($locale) . '.UTF-8');
        
        // Update ICU locale
        \Locale::setDefault($locale);
        
        // Save to session
        $session = Session::getInstance();
        $session->set('locale', $locale);
        
        // Clear loaded translations to force reload with new locale
        $this->loadedDomains = [];
        $this->translations = [];
    }
    
    public function getLocale(): string
    {
        return $this->locale;
    }
    
    public function getAvailableLocales(): array
    {
        return ['en', 'de']; // Supported languages
    }
    
    public function translate(string $key, array $params = [], string $domain = 'messages'): string
    {
        // Load domain if not already loaded
        $this->loadDomainIfNeeded($domain);
        
        // Get translation
        $message = $this->translations[$domain][$key] ?? $key;
        
        // Replace parameters
        if (!empty($params)) {
            foreach ($params as $param => $value) {
                $message = str_replace('{' . $param . '}', $value, $message);
            }
        }
        
        return $message;
    }
    
    public function formatDate(\DateTime $date, string $style = 'medium'): string
    {
        $formatter = new \IntlDateFormatter(
            $this->locale,
            $this->getDateFormatterConstant($style),
            \IntlDateFormatter::NONE
        );
        
        return $formatter->format($date);
    }
    
    public function formatDateTime(\DateTime $dateTime, string $dateStyle = 'medium', string $timeStyle = 'short'): string
    {
        $formatter = new \IntlDateFormatter(
            $this->locale,
            $this->getDateFormatterConstant($dateStyle),
            $this->getDateFormatterConstant($timeStyle)
        );
        
        return $formatter->format($dateTime);
    }
    
    public function formatNumber(float $number, int $decimals = 0): string
    {
        $formatter = new \NumberFormatter($this->locale, \NumberFormatter::DECIMAL);
        $formatter->setAttribute(\NumberFormatter::FRACTION_DIGITS, $decimals);
        
        return $formatter->format($number);
    }
    
    public function formatCurrency(float $amount, string $currency = 'EUR'): string
    {
        $formatter = new \NumberFormatter($this->locale, \NumberFormatter::CURRENCY);
        
        return $formatter->formatCurrency($amount, $currency);
    }
    
    private function loadDomainIfNeeded(string $domain): void
    {
        if (isset($this->loadedDomains[$domain])) {
            return;
        }
        
        // Load translations from JSON file
        $file = APP_ROOT . '/src/Localization/' . $this->locale . '/' . $domain . '.json';
        
        if (file_exists($file)) {
            $json = file_get_contents($file);
            $this->translations[$domain] = json_decode($json, true) ?: [];
        } else {
            // Fallback to English
            $file = APP_ROOT . '/src/Localization/en/' . $domain . '.json';
            if (file_exists($file)) {
                $json = file_get_contents($file);
                $this->translations[$domain] = json_decode($json, true) ?: [];
            } else {
                $this->translations[$domain] = [];
            }
        }
        
        $this->loadedDomains[$domain] = true;
    }
    
    private function getDateFormatterConstant(string $style): int
    {
        return match (strtolower($style)) {
            'none' => \IntlDateFormatter::NONE,
            'short' => \IntlDateFormatter::SHORT,
            'medium' => \IntlDateFormatter::MEDIUM,
            'long' => \IntlDateFormatter::LONG,
            'full' => \IntlDateFormatter::FULL,
            default => \IntlDateFormatter::MEDIUM,
        };
    }
}
