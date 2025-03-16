<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;
use SecretSanta\Services\EmailService;

/**
 * Authentication Controller
 * 
 * Handles all authentication-related actions including login, registration,
 * password reset, and logout functionality.
 * 
 * @package SecretSanta\Controllers
 */
class AuthController extends BaseController
{
    /**
     * Display the login form
     * 
     * If the user is already logged in, redirect to dashboard instead
     * 
     * @return string|void HTML output or redirect
     */
    public function showLogin()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/login');
    }

    /**
     * Process login form submission
     * 
     * Authenticates user credentials and redirects accordingly
     * 
     * @return void
     */
    public function login()
    {
        // Get client IP for rate limiting
        $clientIp = $this->getClientIp();
        
        // Check if IP is on lockout
        if ($this->session->isLoginLocked($clientIp)) {
            $remainingTime = $this->session->getRemainingLockoutTime($clientIp);
            $minutes = ceil($remainingTime / 60);
            $this->session->setFlash('error', t('flash.error.ip_locked', ['minutes' => $minutes]));
            $this->redirect('/auth/login');
            return;
        }
        
        $email = $this->request->getPostParam('email');
        $password = $this->request->getPostParam('password');
        
        if (empty($email) || empty($password)) {
            $this->session->setFlash('error', t('flash.error.email_password_required'));
            $this->redirect('/auth/login');
            return;
        }
        
        // Check if specific email is on lockout
        if ($email && $this->session->isLoginLocked($email)) {
            $remainingTime = $this->session->getRemainingLockoutTime($email);
            $minutes = ceil($remainingTime / 60);
            
            // Don't expose that the email exists, just state that too many attempts were made
            $this->session->setFlash('error', t('flash.error.account_locked', ['minutes' => $minutes]));
            $this->redirect('/auth/login');
            return;
        }
        
        if ($this->auth->login($email, $password)) {
            
            // Successful login - reset login attempts for both IP and email
            $this->session->resetLoginAttempts($email);
            $this->session->resetLoginAttempts($clientIp);
            
            // Get user to access failed login attempts
            $user = $this->auth->user();
            
            // Check for failed login attempts since last successful login
            $failedAttempts = $user->getTempFailedAttempts();

            
            // Check if we have a previous login time stored in flash
            $lastLogin = $this->session->getFlash('last_login');
            if ($lastLogin) {
                $formattedDate = date('d.m.Y H:i', strtotime($lastLogin));
                $this->session->setFlash('success', t('flash.success.welcome_back', [
                    'date' => $formattedDate, 
                    'attempts' => $failedAttempts
                ]));
            } else {
                $this->session->setFlash('success', t('flash.success.logged_in'));
            }
            
            $this->redirect('/user/dashboard');
        } else {
            // Failed login - increment attempts for both IP and email
            $this->session->incrementLoginAttempts($clientIp);
            
            if (!empty($email)) {
                $emailAttempts = $this->session->incrementLoginAttempts($email);
                $remaining = self::MAX_LOGIN_ATTEMPTS - $emailAttempts;
                
                if ($remaining > 0) {
                    $this->session->setFlash('error', t('flash.error.logged_in', ['remaining' => $remaining]));
                } else {
                    $lockoutTime = $this->session->getRemainingLockoutTime($email);
                    $minutes = ceil($lockoutTime / 60);
                    $this->session->setFlash('error', t('flash.error.account_locked', ['minutes' => $minutes]));
                }
            } else {
                $this->session->setFlash('error', t('flash.error.invalid_credentials'));
            }
            
            $this->redirect('/auth/login');
        }
    }

    /**
     * Display the registration form
     * 
     * If the user is already logged in, redirect to dashboard instead
     * 
     * @return string|void HTML output or redirect
     */
    public function showRegister()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/register');
    }

    /**
     * Process registration form submission
     * 
     * Creates a new user account if validation passes and email is not already in use
     * Implements anti-timing attack measures to prevent email enumeration
     * 
     * @return void
     */
    public function register()
    {
        // Get client IP for rate limiting
        $clientIp = $this->getClientIp();
        
        // Check if IP is locked out from registering
        if ($this->session->isRegistrationLocked($clientIp)) {
            $remainingTime = $this->session->getRemainingRegistrationLockoutTime($clientIp);
            $minutes = ceil($remainingTime / 60);
            
            $this->session->setFlash('error', t('flash.error.registration_locked', ['minutes' => $minutes]));
            $this->redirect('/auth/register');
            return;
        }

        $email = $this->request->getPostParam('email');
        $name = $this->request->getPostParam('name');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');
        $mailer = new EmailService();

        // Validate inputs
        $errors = [];

        if (empty($email)) {
            $errors['email'] = 'Email is required';
        } elseif (!filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $errors['email'] = 'Invalid email format';
        }

        if (empty($name)) {
            $errors['name'] = 'Name is required';
        }

        if (empty($password)) {
            $errors['password'] = 'Password is required';
        } elseif (strlen($password) < 8) {
            $errors['password'] = 'Password must be at least 8 characters';
        }

        if ($password !== $passwordConfirm) {
            $errors['password_confirm'] = 'Passwords do not match';
        }

        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old_input', [
                'email' => $email,
                'name' => $name
            ]);
            $this->redirect('/auth/register');
            return;
        }

        // Check if email already exists
        // To mitigate timing attacks, perform the same operations regardless of user existence
        $userRepository = new UserRepository();
        $userExists = $userRepository->findByEmail($email);

        if ($userExists) {
            $mailer->sendExistingAccountNotification($email);
        } else {
            // Create new user
            $this->auth->register($email, $name, $password);
            $mailer->sendWelcomeEmail($email);
        }
        $this->session->incrementRegistrationAttempts($clientIp);
        // Use the same response for both outcomes
        $this->session->setFlash('success', t('flash.success.registration_instructions'));

        // Add a small random delay to make timing analysis more difficult
        usleep(random_int(100000, 200000)); // 0.1-0.2 second delay

        $this->redirect('/auth/login');
    }

    /**
     * Process user logout
     * 
     * Destroys the user session and redirects to homepage
     * 
     * @return void
     */
    public function logout()
    {
        $this->auth->logout();
        $this->session->setFlash('success', t('flash.success.logged_out'));
        $this->redirect('/');
    }

    /**
     * Display the forgot password form
     * 
     * @return string HTML output
     */
    public function showForgotPassword()
    {
        return $this->render('auth/forgot-password');
    }

    /**
     * Process forgot password form submission
     * 
     * Sends password reset instructions to the provided email address
     * Uses consistent responses to prevent email enumeration
     * 
     * @return void
     */
    public function forgotPassword()
    {
        $email = $this->request->getPostParam('email');

        if (empty($email)) {
            $this->session->setFlash('error', t('flash.error.enter_email'));
            $this->redirect('/auth/forgot-password');
            return;
        }

        // Try to send password reset email
        $success = $this->auth->requestPasswordReset($email);

        // Always show the success message (to prevent email enumeration)
        $this->session->setFlash('success', t('flash.success.reset_instructions'));
        $this->redirect('/auth/login');
    }

    /**
     * Display the password reset form
     * 
     * Validates the reset token before showing the form
     * 
     * @param string $token The password reset token
     * @return string|void HTML output or redirect
     */
    public function showResetPassword($token)
    {
        // Check if token is valid before showing the form
        $userRepository = new UserRepository();
        $user = $userRepository->findByResetToken($token);

        // If token doesn't exist or is expired, redirect to forgot password page with error
        if (
            !$user ||
            $user->getResetTokenExpires() === null ||
            strtotime($user->getResetTokenExpires()) < time()
        ) {
            $this->session->setFlash('error', t('flash.error.reset_token_expired'));
            return $this->redirect('/auth/forgot-password');
        }

        return $this->render('auth/reset-password', ['token' => $token]);
    }

    /**
     * Process password reset form submission
     * 
     * Updates user's password if reset token is valid
     * 
     * @return void
     */
    public function resetPassword()
    {
        $token = $this->request->getPostParam('token');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');

        // Validate inputs
        if (empty($token) || empty($password) || empty($passwordConfirm)) {
            $this->session->setFlash('error', t('flash.error.all_fields_required'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if ($password !== $passwordConfirm) {
            $this->session->setFlash('error', t('flash.error.passwords_dont_match'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if (strlen($password) < 8) {
            $this->session->setFlash('error', t('flash.error.password_too_short'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        $success = $this->auth->resetPassword($token, $password);

        if ($success) {
            $this->session->setFlash('success', t('flash.success.password_reset'));
            $this->redirect('/auth/login');
        } else {
            $this->session->setFlash('error', t('flash.error.invalid_reset_token'));
            $this->redirect('/auth/forgot-password');
        }
    }

    /**
     * Get client IP address, taking into account possible proxy servers
     * 
     * @return string Client IP address
     */
    private function getClientIp(): string
    {
        $ip = $_SERVER['REMOTE_ADDR'] ?? '0.0.0.0';
        
        // Check for proxy headers
        $headers = [
            'HTTP_CLIENT_IP',
            'HTTP_X_FORWARDED_FOR',
            'HTTP_X_FORWARDED',
            'HTTP_FORWARDED_FOR',
            'HTTP_FORWARDED'
        ];
        
        foreach ($headers as $header) {
            if (!empty($_SERVER[$header])) {
                $ips = explode(',', $_SERVER[$header]);
                $validIp = trim($ips[0]);
                
                // Make sure it's a valid IP
                if (filter_var($validIp, FILTER_VALIDATE_IP)) {
                    $ip = $validIp;
                    break;
                }
            }
        }
        
        return $ip;
    }
}
