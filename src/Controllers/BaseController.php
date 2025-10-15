<?php

namespace SecretSanta\Controllers;

use SecretSanta\Core\Request;
use SecretSanta\Core\View;
use SecretSanta\Core\Auth;
use SecretSanta\Core\Session;
use SecretSanta\Core\Router;
use SecretSanta\Core\I18n;

/**
 * Base Controller
 * 
 * Abstract parent controller that provides common functionality
 * for all application controllers including rendering, redirection,
 * and authentication checks.
 * 
 * @package SecretSanta\Controllers
 */
class BaseController
{
    /**
     * Core system component instances
     * 
     * @var Request HTTP request handling
     * @var View Template rendering system
     * @var Auth Authentication management
     * @var Session Session state management
     * @var Router URL routing system
     * @var I18n Internationalization service
     */
    protected Request $request;
    protected View $view;
    protected Auth $auth;
    protected Session $session;
    protected Router $router;
    protected I18n $i18n;

    /**
     * Constructor - initializes core system components
     * 
     * Sets up access to request data, authentication, session,
     * routing, internationalization, and view rendering
     */
    public function __construct()
    {
        $this->request = Request::getInstance();
        $this->auth = Auth::getInstance();
        $this->session = Session::getInstance();
        $this->router = Router::getInstance();
        $this->i18n = I18n::getInstance();

        $this->view = new View(__DIR__ . '/../Views');
        $this->session->checkInactivity(); // Ensure the session is updated on each request to reset the inactivity timer
    }

    /**
     * Render a view with the main layout
     * 
     * Renders the specified template within the main layout template,
     * providing common data to all views
     * 
     * @param string $template The template to render
     * @param array $data Data to pass to the template
     * @return string The rendered HTML
     */
    protected function render(string $template, array $data = []): string
    {
        // Add common data for all views
        $data['auth'] = $this->auth;
        $data['session'] = $this->session;
        $data['i18n'] = $this->i18n;

        return $this->view->renderWithLayout($template, 'layouts/main', $data);
    }

    /**
     * Render a partial view without the main layout
     * 
     * Renders a template standalone, useful for AJAX responses
     * or partial HTML fragments
     * 
     * @param string $template The template to render
     * @param array $data Data to pass to the template
     * @return string The rendered HTML
     */
    protected function renderPartial(string $template, array $data = []): string
    {
        // Add i18n to partial views too
        $data['i18n'] = $this->i18n;
        return $this->view->render($template, $data);
    }

    /**
     * Redirect to another URL
     * 
     * Sends a redirect response to the specified URL
     * and terminates the current request
     * 
     * @param string $url The URL to redirect to
     * @return void
     */
    protected function redirect(string $url): void
    {
        $this->router->redirect($url);
    }

    /**
     * Require authentication for a controller action
     * 
     * Checks if user is logged in, redirects to login page if not
     * Stores the intended URL to redirect back after login
     * Useful for protecting routes that require authentication
     * 
     * @return void
     */
    protected function requireAuth(): void
    {
        if (!$this->auth->check()) {
            // Store the intended URL (current request URI)
            $intendedUrl = $this->request->getServerParam('REQUEST_URI', '/');
            $this->session->set('intended_url', $intendedUrl);
            
            $this->session->setFlash('error', t('auth.login.required'));
            $this->redirect('/auth/login');
        }
    }

    /**
     * Return JSON response
     * 
     * Formats an array as JSON, sets appropriate headers
     * and returns the formatted string
     * 
     * @param array $data The data to encode as JSON
     * @param int $status The HTTP status code to send
     * @return string The JSON encoded string
     */
    protected function json(array $data, int $status = 200): string
    {
        http_response_code($status);
        header('Content-Type: application/json');
        return json_encode($data);
    }
}
