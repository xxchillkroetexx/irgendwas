<?php

namespace SecretSanta\Core;

class Autoloader
{
    private array $namespaces = [];

    public function register(): void
    {
        spl_autoload_register([$this, 'loadClass']);
    }

    public function addNamespace(string $namespace, string $baseDir): void
    {
        $namespace = trim($namespace, '\\') . '\\';
        $baseDir = rtrim($baseDir, DIRECTORY_SEPARATOR) . '/';

        $this->namespaces[$namespace] = $baseDir;
    }

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

    public function getRegisteredNamespaces(): array
    {
        return $this->namespaces;
    }
}
