<?php

namespace SecretSanta\Core;

class View {
    private string $templatePath;
    private array $data = [];
    
    public function __construct(string $templatePath = '') {
        $this->templatePath = $templatePath;
    }
    
    public function setTemplatePath(string $templatePath): self {
        $this->templatePath = $templatePath;
        return $this;
    }
    
    public function assign(string $key, $value): self {
        $this->data[$key] = $value;
        return $this;
    }
    
    public function render(string $template, array $data = []): string {
        $this->data = array_merge($this->data, $data);
        
        $templateFile = $this->resolvePath($template);
        
        if (!file_exists($templateFile)) {
            throw new \RuntimeException("Template file not found: $templateFile");
        }
        
        ob_start();
        extract($this->data);
        include $templateFile;
        $output = ob_get_clean();
        
        return $output;
    }
    
    public function renderWithLayout(string $template, string $layout, array $data = []): string {
        $content = $this->render($template, $data);
        
        return $this->render($layout, array_merge($this->data, ['content' => $content]));
    }
    
    public function escape($value): string {
        return htmlspecialchars($value, ENT_QUOTES, 'UTF-8');
    }
    
    public function include(string $template, array $data = []): string {
        return $this->render($template, $data);
    }
    
    private function resolvePath(string $template): string {
        if (strpos($template, '.php') === false) {
            $template .= '.php';
        }
        
        return $this->templatePath . '/' . $template;
    }
}