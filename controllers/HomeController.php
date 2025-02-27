<?php
namespace controllers;

use core\Controller;

class HomeController extends Controller {
    // Show homepage
    public function index() {
        if ($this->isAuthenticated()) {
            // Get current user
            $user = $this->currentUser();
            
            // Get groups user belongs to
            $groups = $user->getGroups();
            
            // Get groups user administers
            $adminGroups = $user->getAdminGroups();
            
            $this->view('home/index', [
                'pageTitle' => 'Dashboard',
                'user' => $user,
                'groups' => $groups,
                'adminGroups' => $adminGroups
            ]);
        } else {
            // Show landing page for guests
            $this->view('home/welcome', [
                'pageTitle' => 'Welcome to Secret Santa'
            ]);
        }
    }
}
