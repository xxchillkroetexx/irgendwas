<?php
namespace utils;

use PHPMailer\PHPMailer\PHPMailer;
use PHPMailer\PHPMailer\Exception;

class EmailService {
    private $mailer;
    
    public function __construct() {
        $this->mailer = new PHPMailer(true);
        
        // Configure SMTP
        $this->mailer->isSMTP();
        $this->mailer->Host = SMTP_HOST;
        $this->mailer->Port = SMTP_PORT;
        $this->mailer->SMTPAuth = true;
        $this->mailer->Username = SMTP_USER;
        $this->mailer->Password = SMTP_PASS;
        $this->mailer->SMTPSecure = 'tls';
        
        // Set default sender
        $this->mailer->setFrom(SMTP_FROM, SMTP_FROM_NAME);
        $this->mailer->isHTML(true);
    }
    
    // Send password reset email
    public function sendPasswordReset($user, $token) {
        // In development, just log the reset link
        $resetLink = APP_URL . '/reset-password/' . $token;
        error_log('Password Reset Link for ' . $user->getEmail());
        
        $subject = 'Password Reset - ' . APP_NAME;
        $body = $this->getEmailTemplate('password_reset', [
            'name' => $user->getFirstName(),
            'reset_link' => $resetLink,
            'app_name' => APP_NAME
        ]);
        
        return $this->send($user->getEmail(), $subject, $body);
    }
    
    // Send group invitation
    public function sendInvitation($email, $group, $token, $inviter) {
        $inviteLink = APP_URL . '/invitation/' . $token;
        
        $subject = 'You\'ve been invited to a Secret Santa group!';
        $body = $this->getEmailTemplate('group_invitation', [
            'inviter_name' => $inviter->getFullName(),
            'group_name' => $group->getName(),
            'invite_link' => $inviteLink,
            'app_name' => APP_NAME
        ]);
        
        return $this->send($email, $subject, $body);
    }
    
    // Send assignment notification
    public function sendAssignmentNotification($user, $recipient, $group) {
        // Check for custom email template
        $template = $group->getCustomEmailTemplate();
        $useCustomTemplate = !empty($template);
        
        $subject = 'Your Secret Santa Assignment';
        
        if ($useCustomTemplate) {
            // Replace placeholders in custom template
            $body = str_replace(
                ['{{recipient_name}}', '{{group_name}}'],
                [$recipient->getFullName(), $group->getName()],
                $template
            );
        } else {
            // Use default template
            $body = $this->getEmailTemplate('assignment', [
                'name' => $user->getFirstName(),
                'recipient_name' => $recipient->getFullName(),
                'group_name' => $group->getName(),
                'wishlist_link' => APP_URL . '/groups/' . $group->getId() . '/wishlist/' . $recipient->getId(),
                'app_name' => APP_NAME
            ]);
        }
        
        return $this->send($user->getEmail(), $subject, $body);
    }
    
    // Send wishlist reminder
    public function sendWishlistReminder($recipient, $group) {
        $subject = 'Reminder: Add items to your Secret Santa wishlist';
        $body = $this->getEmailTemplate('wishlist_reminder', [
            'name' => $recipient->getFirstName(),
            'group_name' => $group->getName(),
            'wishlist_link' => APP_URL . '/groups/' . $group->getId() . '/wishlist',
            'app_name' => APP_NAME
        ]);
        
        return $this->send($recipient->getEmail(), $subject, $body);
    }
    
    // Get email template
    private function getEmailTemplate($template, $variables) {
        $templatePath = ROOT_PATH . '/views/emails/' . $template . '.html';
        
        if (file_exists($templatePath)) {
            $content = file_get_contents($templatePath);
        } else {
            // Default templates if file doesn't exist
            switch ($template) {
                case 'password_reset':
                    $content = '
                    <h1>Password Reset</h1>
                    <p>Hello {{name}},</p>
                    <p>You requested a password reset for your {{app_name}} account. Click the link below to set a new password:</p>
                    <p><a href="{{reset_link}}">Reset Password</a></p>
                    <p>If you didn\'t request this, you can ignore this email.</p>
                    <p>Thanks,<br>{{app_name}} Team</p>';
                    break;
                case 'group_invitation':
                    $content = '
                    <h1>Secret Santa Invitation</h1>
                    <p>Hello there!</p>
                    <p>{{inviter_name}} has invited you to join their Secret Santa group "{{group_name}}" on {{app_name}}.</p>
                    <p>Click the link below to join:</p>
                    <p><a href="{{invite_link}}">Accept Invitation</a></p>
                    <p>Thanks,<br>{{app_name}} Team</p>';
                    break;
                case 'assignment':
                    $content = '
                    <h1>Your Secret Santa Assignment</h1>
                    <p>Hello {{name}},</p>
                    <p>Your Secret Santa assignment for "{{group_name}}" is ready!</p>
                    <p>You will be the Secret Santa for: <strong>{{recipient_name}}</strong></p>
                    <p>You can <a href="{{wishlist_link}}">view their wishlist here</a>.</p>
                    <p>Happy gifting!</p>
                    <p>Thanks,<br>{{app_name}} Team</p>';
                    break;
                case 'wishlist_reminder':
                    $content = '
                    <h1>Don\'t Forget Your Wishlist</h1>
                    <p>Hello {{name}},</p>
                    <p>This is a friendly reminder to add items to your wishlist for "{{group_name}}".</p>
                    <p>Your Secret Santa will appreciate having some ideas for what to get you!</p>
                    <p><a href="{{wishlist_link}}">Update your wishlist now</a></p>
                    <p>Thanks,<br>{{app_name}} Team</p>';
                    break;
                default:
                    $content = '<p>Email content not available.</p>';
            }
        }
        
        // Replace template variables
        foreach ($variables as $key => $value) {
            $content = str_replace('{{' . $key . '}}', $value, $content);
        }
        
        return $content;
    }
    
    // Send email
    private function send($to, $subject, $body) {
        try {
            $this->mailer->clearAddresses();
            $this->mailer->addAddress($to);
            $this->mailer->Subject = $subject;
            $this->mailer->Body = $body;
            $this->mailer->AltBody = strip_tags($body);
            
            return $this->mailer->send();
        } catch (Exception $e) {
            error_log('Email sending failed: ' . $this->mailer->ErrorInfo);
            return false;
        }
    }
}
