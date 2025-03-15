<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;
use SecretSanta\Services\EmailService;

class AuthController extends BaseController
{
    // Die Konstante sollte mit der aus Session.php übereinstimmen
    private const MAX_LOGIN_ATTEMPTS = 5;

    public function showLogin()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/login');
    }

    public function login()
    {
        // Get client IP for rate limiting
        $clientIp = $this->getClientIp();
        
        // Check if IP is on lockout
        if ($this->session->isLoginLocked($clientIp)) {
            $remainingTime = $this->session->getRemainingLockoutTime($clientIp);
            $minutes = ceil($remainingTime / 60);
            $this->session->setFlash('error', "Zu viele fehlgeschlagene Login-Versuche von dieser IP-Adresse. Bitte versuchen Sie es nach $minutes Minuten erneut.");
            $this->redirect('/auth/login');
            return;
        }
        
        $email = $this->request->getPostParam('email');
        $password = $this->request->getPostParam('password');
        
        if (empty($email) || empty($password)) {
            $this->session->setFlash('error', 'Bitte geben Sie sowohl E-Mail als auch Passwort ein');
            $this->redirect('/auth/login');
            return;
        }
        
        // Check if specific email is on lockout
        if ($email && $this->session->isLoginLocked($email)) {
            $remainingTime = $this->session->getRemainingLockoutTime($email);
            $minutes = ceil($remainingTime / 60);
            
            // Don't expose that the email exists, just state that too many attempts were made
            $this->session->setFlash('error', "Zu viele fehlgeschlagene Login-Versuche. Bitte versuchen Sie es nach $minutes Minuten erneut.");
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
               $this->session->setFlash('success', "Willkommen zurück! Ihr letzter Login war am {$formattedDate}. Seit Ihrem letzten erfolgreichen Login gab es {$failedAttempts} fehlgeschlagene Anmeldeversuche für Ihr Konto.");
            } else {
                $this->session->setFlash('success', 'Sie haben sich erfolgreich angemeldet.');
            }
            
            $this->redirect('/user/dashboard');
        } else {
            // Failed login - increment attempts for both IP and email
            $this->session->incrementLoginAttempts($clientIp);
            
            if (!empty($email)) {
                $emailAttempts = $this->session->incrementLoginAttempts($email);
                $remaining = self::MAX_LOGIN_ATTEMPTS - $emailAttempts;
                
                if ($remaining > 0) {
                    $this->session->setFlash('error', "Ungültige E-Mail oder Passwort. Verbleibende Versuche: $remaining");
                } else {
                    $lockoutTime = $this->session->getRemainingLockoutTime($email);
                    $minutes = ceil($lockoutTime / 60);
                    $this->session->setFlash('error', "Zu viele fehlgeschlagene Login-Versuche. Ihr Konto ist für $minutes Minuten gesperrt.");
                }
            } else {
                $this->session->setFlash('error', 'Ungültige E-Mail oder Passwort.');
            }
            
            $this->redirect('/auth/login');
        }
    }

    public function showRegister()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/register');
    }

    public function register()
    {
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

        // Use the same response for both outcomes
        $this->session->setFlash('success', 'If your registration was successful, you will receive an email with instructions');

        // Add a small random delay to make timing analysis more difficult
        usleep(random_int(100000, 200000)); // 0.1-0.2 second delay

        $this->redirect('/auth/login');
    }

    public function logout()
    {
        $this->auth->logout();
        $this->session->setFlash('success', 'You have been successfully logged out');
        $this->redirect('/');
    }

    public function showForgotPassword()
    {
        return $this->render('auth/forgot-password');
    }

    public function forgotPassword()
    {
        $email = $this->request->getPostParam('email');

        if (empty($email)) {
            $this->session->setFlash('error', 'Please enter your email address');
            $this->redirect('/auth/forgot-password');
            return;
        }

        // Try to send password reset email
        $success = $this->auth->requestPasswordReset($email);

        // Always show the success message (to prevent email enumeration)
        $this->session->setFlash('success', 'If your email exists in our system, you will receive password reset instructions');
        $this->redirect('/auth/login');
    }

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

            $this->session->setFlash('error', 'The password reset link has expired or is invalid. Please request a new one.');
            return $this->redirect('/auth/forgot-password');
        }

        return $this->render('auth/reset-password', ['token' => $token]);
    }

    public function resetPassword()
    {
        $token = $this->request->getPostParam('token');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');

        // Validate inputs
        if (empty($token) || empty($password) || empty($passwordConfirm)) {
            $this->session->setFlash('error', 'All fields are required');
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if ($password !== $passwordConfirm) {
            $this->session->setFlash('error', 'Passwords do not match');
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if (strlen($password) < 8) {
            $this->session->setFlash('error', 'Password must be at least 8 characters');
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        $success = $this->auth->resetPassword($token, $password);

        if ($success) {
            $this->session->setFlash('success', 'Your password has been reset, you can now login');
            $this->redirect('/auth/login');
        } else {
            $this->session->setFlash('error', 'Invalid or expired reset token');
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
