<?php

namespace SecretSanta\Core;

/**
 * View class for template rendering
 * 
 * Handles rendering of template files, including layouts,
 * template inclusion, and data assignment.
 */
class View
{
    /**
     * @var string Path to the template directory
     */
    private string $templatePath;
    
    /**
     * @var array Data to be passed to the template
     */
    private array $data = [];

    /**
     * Constructor
     * 
     * @param string $templatePath Path to the template directory
     */
    public function __construct(string $templatePath = '')
    {
        $this->templatePath = $templatePath;
    }

    /**
     * Set the template directory path
     * 
     * @param string $templatePath Path to the template directory
     * @return self For method chaining
     */
    public function setTemplatePath(string $templatePath): self
    {
        $this->templatePath = $templatePath;
        return $this;
    }

    /**
     * Assign a value to a template variable
     * 
     * @param string $key Variable name
     * @param mixed $value Variable value
     * @return self For method chaining
     */
    public function assign(string $key, $value): self
    {
        $this->data[$key] = $value;
        return $this;
    }

    /**
     * Render a template with provided data
     * 
     * @param string $template Template file name
     * @param array $data Additional data to be passed to the template
     * @return string Rendered template output
     * @throws \RuntimeException If template file not found
     */
    public function render(string $template, array $data = []): string
    {
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

    /**
     * Render a template within a layout
     * 
     * @param string $template Content template file name
     * @param string $layout Layout template file name
     * @param array $data Additional data to be passed to the templates
     * @return string Rendered output with content in layout
     */
    public function renderWithLayout(string $template, string $layout, array $data = []): string
    {
        $content = $this->render($template, $data);

        return $this->render($layout, array_merge($this->data, ['content' => $content]));
    }

    /**
     * HTML escape a value for safe output
     * 
     * @param mixed $value Value to be escaped
     * @return string Escaped value
     */
    public function escape($value): string
    {
        return htmlspecialchars($value, ENT_QUOTES, 'UTF-8');
    }

    /**
     * Include another template file
     * 
     * @param string $template Template file to include
     * @param array $data Data to be passed to the included template
     * @return string Rendered output of included template
     */
    public function include(string $template, array $data = []): string
    {
        return $this->render($template, $data);
    }

    /**
     * Resolve a template name to its full file path
     * 
     * @param string $template Template name
     * @return string Full path to the template file
     */
    private function resolvePath(string $template): string
    {
        if (strpos($template, '.php') === false) {
            $template .= '.php';
        }

        return $this->templatePath . '/' . $template;
    }
}
