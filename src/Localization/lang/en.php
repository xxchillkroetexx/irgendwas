<?php

return [
    'app' => [
        'name' => 'Secret Santa',
        'tagline' => 'Make gift-giving fun and easy'
    ],
    'auth' => [
        'login' => 'Login',
        'register' => 'Register',
        'logout' => 'Logout',
        'email' => 'Email',
        'password' => 'Password',
        'confirm_password' => 'Confirm Password',
        'name' => 'Name',
        'remember_me' => 'Remember Me',
        'forgot_password' => 'Forgot Password?',
        'reset_password' => 'Reset Password',
        'submit' => 'Submit',
        'already_have_account' => 'Already have an account?',
        'dont_have_account' => 'Don\'t have an account?',
        'welcome_back' => 'Welcome back! Your last login was on {date}.',
        'login_success' => 'You have successfully logged in.'
    ],
    'groups' => [
        'my_groups' => 'My Groups',
        'create_group' => 'Create Group',
        'join_group' => 'Join Group',
        'group_name' => 'Group Name',
        'group_description' => 'Group Description',
        'admin' => 'Admin',
        'created_at' => 'Created At',
        'registration_deadline' => 'Registration Deadline',
        'draw_date' => 'Draw Date',
        'invitation_code' => 'Invitation Code',
        'members' => 'Members',
        'add_member' => 'Add Member',
        'remove_member' => 'Remove Member',
        'perform_draw' => 'Perform Draw',
        'draw_completed' => 'Draw Completed',
        'draw_pending' => 'Draw Pending',
        'invite_participants' => 'Invite Participants',
        'participant_email' => 'Participant Email',
        'invite' => 'Invite',
        'view_details' => 'View Details',
        'edit_group' => 'Edit Group',
        'delete_group' => 'Delete Group',
        'confirm_delete' => 'Are you sure you want to delete this group?'
    ],
    'wishlists' => [
        'my_wishlist' => 'My Wishlist',
        'view_wishlist' => 'View Wishlist',
        'edit_wishlist' => 'Edit Wishlist',
        'add_item' => 'Add Item',
        'edit_item' => 'Edit Item',
        'delete_item' => 'Delete Item',
        'item_title' => 'Title',
        'item_description' => 'Description',
        'item_link' => 'Link',
        'priority_ordered' => 'This wishlist is ordered by priority',
        'save_changes' => 'Save Changes',
        'move_up' => 'Move Up',
        'move_down' => 'Move Down'
    ],
    'assignments' => [
        'your_recipient' => 'Your Gift Recipient',
        'view_wishlist' => 'View Wishlist',
        'assignment_date' => 'Assignment Date'
    ],
    'exclusions' => [
        'manage_exclusions' => 'Manage Exclusions',
        'add_exclusion' => 'Add Exclusion',
        'user' => 'User',
        'excluded_user' => 'Cannot give to',
        'delete_exclusion' => 'Delete Exclusion'
    ],
    'notifications' => [
        'invitation_subject' => 'You\'ve been invited to join a Secret Santa group',
        'draw_subject' => 'Your Secret Santa Draw Results',
        'password_reset_subject' => 'Password Reset Request'
    ],
    'errors' => [
        'invalid_credentials' => 'Invalid email or password',
        'email_taken' => 'This email is already registered',
        'passwords_dont_match' => 'Passwords do not match',
        'group_not_found' => 'Group not found',
        'user_not_found' => 'User not found',
        'wishlist_not_found' => 'Wishlist not found',
        'not_authorized' => 'You are not authorized to perform this action',
        'invalid_invitation_code' => 'Invalid invitation code',
        'already_member' => 'You are already a member of this group',
        'draw_already_completed' => 'The draw has already been completed',
        'not_enough_members' => 'At least 2 members are required for a draw',
        'no_assignment' => 'You have not been assigned a recipient yet'
    ],
    'success' => [
        'login_success' => 'You have successfully logged in',
        'register_success' => 'You have successfully registered',
        'logout_success' => 'You have successfully logged out',
        'group_created' => 'Group has been created successfully',
        'group_updated' => 'Group has been updated successfully',
        'group_deleted' => 'Group has been deleted successfully',
        'joined_group' => 'You have successfully joined the group',
        'invitation_sent' => 'Invitation has been sent successfully',
        'draw_completed' => 'The draw has been completed successfully',
        'wishlist_updated' => 'Wishlist has been updated successfully',
        'password_reset_link_sent' => 'Password reset link has been sent to your email',
        'password_reset_success' => 'Your password has been reset successfully'
    ]
];
