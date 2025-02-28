<?php

namespace SecretSanta\Localization;

use SecretSanta\Core\Session;

class Translator {
    private static ?self $instance = null;
    private string $defaultLocale = 'en';
    private string $currentLocale;
    private array $translations = [];
    
    private function __construct() {
        $session = Session::getInstance();
        $this->currentLocale = $session->get('locale', $this->defaultLocale);
        $this->loadTranslations();
    }
    
    public static function getInstance(): self {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    public function translate(string $key, array $replacements = []): string {
        $keys = explode('.', $key);
        $value = $this->translations;
        
        foreach ($keys as $part) {
            if (!isset($value[$part])) {
                return $key; // Translation not found, return the key
            }
            $value = $value[$part];
        }
        
        if (!is_string($value)) {
            return $key; // Translation is not a string, return the key
        }
        
        // Replace placeholders with values
        foreach ($replacements as $placeholder => $replacement) {
            $value = str_replace(":{$placeholder}", $replacement, $value);
        }
        
        return $value;
    }
    
    public function getLocale(): string {
        return $this->currentLocale;
    }
    
    public function setLocale(string $locale): void {
        if (!in_array($locale, $this->getAvailableLocales())) {
            $locale = $this->defaultLocale;
        }
        
        $this->currentLocale = $locale;
        Session::getInstance()->set('locale', $locale);
        $this->loadTranslations();
    }
    
    public function getAvailableLocales(): array {
        return ['en', 'de']; // Currently supported languages
    }
    
    private function loadTranslations(): void {
        $file = __DIR__ . "/lang/{$this->currentLocale}.php";
        
        if (file_exists($file)) {
            $this->translations = include $file;
        } else {
            $this->translations = [];
        }
    }
}