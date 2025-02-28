<?php
namespace controllers;

use core\Controller;
use models\User;
use utils\EmailService;

class AuthController extends Controller {
    // Show login form
    public function showLogin() {
        // Redirect if already logged in
        if ($this->isAuthenticated()) {
            $this->redirect('/');
        }
        
        $this->view('auth/login', [
            'pageTitle' => 'Login',
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Process login form
    public function login() {
        // Validate CSRF token
        $this->validateCSRF();
        
        $email = $_POST['email'] ?? '';
        $password = $_POST['password'] ?? '';
        
        // Validate input
        $errors = [];
        
        if (empty($email)) {
            $errors['email'] = 'Email is required';
        } elseif (!filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $errors['email'] = 'Invalid email format';
        }
        
        if (empty($password)) {
            $errors['password'] = 'Password is required';
        }
        
        // If validation fails, show login form with errors
        if (!empty($errors)) {
            $this->view('auth/login', [
                'pageTitle' => 'Login',
                'csrf' => $this->generateCSRF(),
                'errors' => $errors,
                'email' => $email
            ]);
            return;
        }
        
        // Attempt to authenticate user
        $userModel = new User();
        $user = $userModel->findByEmail($email);
        
        if (!$user || !$user->verifyPassword($password)) {
            $this->flash('danger', 'Invalid email or password');
            $this->view('auth/login', [
                'pageTitle' => 'Login',
                'csrf' => $this->generateCSRF(),
                'email' => $email
            ]);
            return;
        }
        
        // Set user session
        $_SESSION['user_id'] = $user->getId();
        
        // Redirect to intended page or home
        $redirect = $_SESSION['redirect_after_login'] ?? '/';
        unset($_SESSION['redirect_after_login']);
        
        $this->flash('success', 'Login successful, welcome ' . $user->getFirstName() . '!');
        $this->redirect($redirect);
    }
    
    // Show registration form
    public function showRegister() {
        // Redirect if already logged in
        if ($this->isAuthenticated()) {
            $this->redirect('/');
        }
        
        $this->view('auth/register', [
            'pageTitle' => 'Register',
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Process registration form
    public function register() {
        // Validate CSRF token
        $this->validateCSRF();
        
        // Get form data
        $email = $_POST['email'] ?? '';
        $password = $_POST['password'] ?? '';
        $password_confirm = $_POST['password_confirm'] ?? '';
        $firstName = $_POST['first_name'] ?? '';
        $lastName = $_POST['last_name'] ?? '';
        
        // Basic validation
        $errors = [];
        
        if (empty($email) || !filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $errors['email'] = 'Valid email is required';
        }
        
        if (empty($password) || strlen($password) < 8) {
            $errors['password'] = 'Password must be at least 8 characters';
        }
        
        if ($password !== $password_confirm) {
            $errors['password_confirm'] = 'Passwords do not match';
        }
        
        if (empty($firstName)) {
            $errors['first_name'] = 'First name is required';
        }
        
        if (empty($lastName)) {
            $errors['last_name'] = 'Last name is required';
        }
        
        // Check if email is already in use
        $userModel = new User();
        if (empty($errors['email']) && $userModel->findByEmail($email)) {
            $errors['email'] = 'Email is already registered';
        }
        
        // If validation fails, show registration form with errors
        if (!empty($errors)) {
            $this->view('auth/register', [
                'pageTitle' => 'Register',
                'csrf' => $this->generateCSRF(),
                'errors' => $errors,
                'email' => $email,
                'first_name' => $firstName,
                'last_name' => $lastName
            ]);
            return;
        }
        
        // Create user
        $user = $userModel->create([
            'email' => $email,
            'password' => $password,
            'first_name' => $firstName,
            'last_name' => $lastName
        ]);
        
        // Log user in
        $_SESSION['user_id'] = $user->getId();
        
        // Redirect to home page
        $this->flash('success', 'Registration successful! Welcome to Secret Santa.');
        $this->redirect('/');
    }
    
    // Log user out
    public function logout() {
        // Clear session variables
        unset($_SESSION['user_id']);
        
        // Destroy session
        session_destroy();
        
        // Redirect to login page
        $this->redirect('/login');
    }

    // Show forgot password form
    public function showForgotPassword() {
        // Redirect if already logged in
        if ($this->isAuthenticated()) {
            $this->redirect('/');
        }
        
        $this->view('auth/forgot_password', [
            'pageTitle' => 'Forgot Password',
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Process forgot password form
    public function forgotPassword() {
        // Validate CSRF token
        $this->validateCSRF();
        
        $email = $_POST['email'] ?? '';
        
        // Validate input
        if (empty($email) || !filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $this->view('auth/forgot_password', [
                'pageTitle' => 'Forgot Password',
                'csrf' => $this->generateCSRF(),
                'error' => 'Please enter a valid email address',
                'email' => $email
            ]);
            return;
        }
        
        // Find user by email
        $userModel = new User();
        $user = $userModel->findByEmail($email);
        
        // Generate reset token even if user doesn't exist (prevents email enumeration)
        if ($user) {
            $token = $user->generateResetToken();
            
            // Send password reset email
            try {
                $emailService = new EmailService();
                $emailService->sendPasswordReset($user, $token);
            } catch (\Exception $e) {
                // Log the error but don't expose it to the user
                error_log('Failed to send password reset email: ' . $e->getMessage());
            }
        }
        
        // Always show success message (prevents email enumeration)
        $this->flash('success', 'If that email address is in our system, you will receive a password reset link shortly');
        $this->redirect('/login');
    }
    
    // Show reset password form
    public function showResetPassword($token) {
        // Validate token
        $userModel = new User();
        $user = $userModel->findByResetToken($token);
        
        if (!$user) {
            $this->flash('danger', 'Invalid or expired password reset token');
            $this->redirect('/forgot-password');
            return;
        }
        
        $this->view('auth/reset_password', [
            'pageTitle' => 'Reset Password',
            'csrf' => $this->generateCSRF(),
            'token' => $token
        ]);
    }
    
    // Process reset password form
    public function resetPassword() {
        // Validate CSRF token
        $this->validateCSRF();
        
        $token = $_POST['token'] ?? '';
        $password = $_POST['password'] ?? '';
        $password_confirm = $_POST['password_confirm'] ?? '';
        
        // Validate token
        $userModel = new User();
        $user = $userModel->findByResetToken($token);
        
        if (!$user) {
            $this->flash('danger', 'Invalid or expired password reset token');
            $this->redirect('/forgot-password');
            return;
        }
        
        // Validate password
        $errors = [];
        
        if (empty($password)) {
            $errors['password'] = 'Password is required';
        } elseif (strlen($password) < 8) {
            $errors['password'] = 'Password must be at least 8 characters long';
        }
        
        if ($password !== $password_confirm) {
            $errors['password_confirm'] = 'Passwords do not match';
        }
        
        if (!empty($errors)) {
            $this->view('auth/reset_password', [
                'pageTitle' => 'Reset Password',
                'csrf' => $this->generateCSRF(),
                'token' => $token,
                'errors' => $errors
            ]);
            return;
        }
        
        // Update password and clear reset token
        $user->update(['password' => $password]);
        $user->clearResetToken();
        
        $this->flash('success', 'Password has been reset successfully. You can now log in with your new password.');
        $this->redirect('/login');
    }
}
