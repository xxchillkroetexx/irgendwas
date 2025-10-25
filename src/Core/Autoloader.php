<?php

namespace SecretSanta\Core;

/**
 * Autoloader class provides PSR-4 compliant class autoloading.
 * 
 * Allows registering namespaces with corresponding base directories
 * and automatically loads class files when needed.
 */
class Autoloader
{
    /** @var array Registered namespaces with their base directories */
    private array $namespaces = [];

    /**
     * Registers this autoloader with PHP's autoload system.
     */
    public function register(): void
    {
        spl_autoload_register([$this, 'loadClass']);
    }

    /**
     * Adds a namespace prefix to the autoloader.
     * 
     * @param string $namespace The namespace prefix
     * @param string $baseDir The base directory for the namespace
     */
    public function addNamespace(string $namespace, string $baseDir): void
    {
        $namespace = trim($namespace, '\\') . '\\';
        $baseDir = rtrim($baseDir, DIRECTORY_SEPARATOR) . '/';

        $this->namespaces[$namespace] = $baseDir;
    }

    /**
     * Loads a class file based on its fully qualified name.
     * 
     * @param string $class The fully qualified class name
     * @return bool True if the class was loaded, false otherwise
     */
    public function loadClass(string $class): bool
    {
        $namespace = $class;

        // Try loading from registered namespaces
        while (false !== $pos = strrpos($namespace, '\\')) {
            $namespace = substr($class, 0, $pos + 1);
            $relativeClass = substr($class, $pos + 1);

            if ($this->loadMappedFile($namespace, $relativeClass)) {
                return true;
            }

            // Remove trailing namespace separator for the next iteration
            $namespace = rtrim($namespace, '\\');
        }

        // Try to use standard PSR-4 autoloading
        $fileName = str_replace('\\', '/', $class) . '.php';

        if (file_exists($fileName)) {
            require_once $fileName;
            return true;
        }

        // If we reach here, the class wasn't found
        return false;
    }

    /**
     * Loads a class file from a registered namespace.
     * 
     * @param string $namespace The namespace prefix
     * @param string $relativeClass The class name relative to the namespace
     * @return bool True if the file was loaded, false otherwise
     */
    private function loadMappedFile(string $namespace, string $relativeClass): bool
    {
        if (!isset($this->namespaces[$namespace])) {
            return false;
        }

        $baseDir = $this->namespaces[$namespace];
        $file = $baseDir . str_replace('\\', '/', $relativeClass) . '.php';

        if (file_exists($file)) {
            require_once $file;
            return true;
        }

        return false;
    }

    /**
     * Gets the list of registered namespaces and their base directories.
     * 
     * @return array The registered namespaces
     */
    public function getRegisteredNamespaces(): array
    {
        return $this->namespaces;
    }
}
